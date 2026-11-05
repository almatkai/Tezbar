#[cfg(feature = "bench-control")]
mod bench_control;
mod launcher_view;
mod search_session;
mod text_input;
mod theme;
use app_lib::launcher_facade::{Launcher, PrototypeProfile};
use gpui::*;
use std::{borrow::Cow, path::PathBuf};
use text_input::*;
struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "search.svg" => Some(Cow::Borrowed(include_bytes!("../assets/search.svg"))),
            "app.svg" => Some(Cow::Borrowed(include_bytes!("../assets/app.svg"))),
            "file.svg" => Some(Cow::Borrowed(include_bytes!("../assets/file.svg"))),
            "note.svg" => Some(Cow::Borrowed(include_bytes!("../assets/note.svg"))),
            "key.svg" => Some(Cow::Borrowed(include_bytes!("../assets/key.svg"))),
            "puzzle.svg" => Some(Cow::Borrowed(include_bytes!("../assets/puzzle.svg"))),
            "snippet.svg" => Some(Cow::Borrowed(include_bytes!("../assets/snippet.svg"))),
            "link.svg" => Some(Cow::Borrowed(include_bytes!("../assets/link.svg"))),
            "command.svg" => Some(Cow::Borrowed(include_bytes!("../assets/command.svg"))),
            _ => None,
        })
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
struct RuntimeOwner {
    _runtime: tokio::runtime::Runtime,
    #[cfg(feature = "bench-control")]
    _server: Option<bench_control::Server>,
}
impl Global for RuntimeOwner {}
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--version") {
        println!(
            "{}",
            serde_json::json!({"source":env!("TEZBAR_PROTOTYPE_GIT_SHA"),"dirty":env!("TEZBAR_PROTOTYPE_DIRTY"),"gpui_rev":"c84af5b99cbc366d675064e52b23664a015b6e35","runtime_shaders":true,"bench_control":cfg!(feature="bench-control")})
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--help") {
        println!(
            "--profile <marked OS-temp root> --resources <matching read-only Resources> [--show] [--headless] [--theme light|dark|system]\nbench-control only: --control-token-file <profile-local file>"
        );
        return Ok(());
    }
    theme::set(value("--theme").as_deref().unwrap_or("system")).map_err(anyhow::Error::msg)?;
    let root =
        PathBuf::from(value("--profile").ok_or_else(|| anyhow::anyhow!("--profile required"))?)
            .canonicalize()?;
    let resources =
        PathBuf::from(value("--resources").ok_or_else(|| anyhow::anyhow!("--resources required"))?);
    anyhow::ensure!(
        std::fs::read_to_string(root.join(".tezbar-native-launcher-prototype"))?
            == env!("TEZBAR_PROTOTYPE_GIT_SHA"),
        "fixture source SHA differs from binary"
    );
    let profile = PrototypeProfile::open(&root, &resources).map_err(anyhow::Error::msg)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let core = {
        let _entered = runtime.enter();
        Launcher::start(&profile).map_err(anyhow::Error::msg)?
    };
    #[cfg(feature = "bench-control")]
    let (server, mut receiver) = {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let server = if let Some(path) = value("--control-token-file") {
            let path = PathBuf::from(path).canonicalize()?;
            anyhow::ensure!(
                path.starts_with(&root),
                "control token file must be inside profile"
            );
            let token = std::fs::read_to_string(path)?.trim().to_owned();
            let _entered = runtime.enter();
            Some(
                bench_control::Server::start(root.clone(), token, tx)
                    .map_err(anyhow::Error::msg)?,
            )
        } else {
            None
        };
        (server, rx)
    };
    if args.iter().any(|a| a == "--headless") {
        runtime.block_on(async {
            core.ready().await.map_err(anyhow::Error::msg)?;
            println!("{}",serde_json::json!({"phase":"core-ready","mode":"headless-C","pid":std::process::id()}));
            #[cfg(feature="bench-control")]
            if server.is_some() {
                while let Some(command)=receiver.recv().await {
                    let op=command.request["op"].as_str().unwrap_or("");
                    let result=match op {
                        "ready"|"status"=>core.status().await.map(|status|serde_json::json!({"phase":"core-ready","mode":"headless-C","launcher":status,"pid":std::process::id()})),
                        "query"=>core.search(command.request["text"].as_str().unwrap_or("")).await.map(|rows|serde_json::json!({"result_count":rows.len()})),
                        "quit"=>Ok(serde_json::Value::Null),_=>Err("headless mode supports ready, status, query, quit".into()),
                    };
                    let _=command.reply.send(result);if op=="quit"{break;}
                }
            } else {tokio::signal::ctrl_c().await?;}
            #[cfg(not(feature="bench-control"))] tokio::signal::ctrl_c().await?;
            core.shutdown().await;Ok::<_,anyhow::Error>(())
        })?;
        #[cfg(feature = "bench-control")]
        drop(server);
        runtime.shutdown_timeout(std::time::Duration::from_secs(5));
        return Ok(());
    }
    let handle = runtime.handle().clone();
    let enter_handle = handle.clone();
    let _entered = enter_handle.enter();
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            gpui_tokio::init_from_handle(cx, handle.clone());
            cx.set_activation_policy(ActivationPolicy::Accessory);
            cx.set_global(RuntimeOwner {
                _runtime: runtime,
                #[cfg(feature = "bench-control")]
                _server: server,
            });
            cx.bind_keys([
                KeyBinding::new("backspace", Backspace, Some("TextInput")),
                KeyBinding::new("delete", Delete, Some("TextInput")),
                KeyBinding::new("left", Left, Some("TextInput")),
                KeyBinding::new("right", Right, Some("TextInput")),
                KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
                KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-a"
                    } else {
                        "ctrl-a"
                    },
                    SelectAll,
                    Some("TextInput"),
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-c"
                    } else {
                        "ctrl-c"
                    },
                    Copy,
                    Some("TextInput"),
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-x"
                    } else {
                        "ctrl-x"
                    },
                    Cut,
                    Some("TextInput"),
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-v"
                    } else {
                        "ctrl-v"
                    },
                    Paste,
                    Some("TextInput"),
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "alt-left"
                    } else {
                        "ctrl-left"
                    },
                    WordLeft,
                    Some("TextInput"),
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "alt-right"
                    } else {
                        "ctrl-right"
                    },
                    WordRight,
                    Some("TextInput"),
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "alt-shift-left"
                    } else {
                        "ctrl-shift-left"
                    },
                    SelectWordLeft,
                    Some("TextInput"),
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "alt-shift-right"
                    } else {
                        "ctrl-shift-right"
                    },
                    SelectWordRight,
                    Some("TextInput"),
                ),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "alt-backspace"
                    } else {
                        "ctrl-backspace"
                    },
                    DeleteWord,
                    Some("TextInput"),
                ),
                KeyBinding::new("home", Home, Some("TextInput")),
                KeyBinding::new("end", End, Some("TextInput")),
                KeyBinding::new("shift-home", SelectHome, Some("TextInput")),
                KeyBinding::new("shift-end", SelectEnd, Some("TextInput")),
                KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("TextInput")),
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-q"
                    } else {
                        "alt-f4"
                    },
                    launcher_view::Quit,
                    None,
                ),
            ]);
            #[cfg(target_os = "macos")]
            cx.bind_keys([
                KeyBinding::new("cmd-left", Home, Some("TextInput")),
                KeyBinding::new("cmd-right", End, Some("TextInput")),
                KeyBinding::new("cmd-shift-left", SelectHome, Some("TextInput")),
                KeyBinding::new("cmd-shift-right", SelectEnd, Some("TextInput")),
            ]);
            let show = args.iter().any(|a| a == "--show");
            #[cfg(feature = "bench-control")]
            let control_core = core.clone();
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(px(760.), px(640.)),
                            cx,
                        ))),
                        titlebar: None,
                        show,
                        focus: show,
                        is_resizable: false,
                        is_minimizable: false,
                        window_background: if cfg!(target_os = "macos") {
                            WindowBackgroundAppearance::Blurred
                        } else {
                            WindowBackgroundAppearance::Opaque
                        },
                        app_id: Some("com.tezbar.prototype.nativelauncher".into()),
                        ..Default::default()
                    },
                    |w, cx| cx.new(|cx| launcher_view::LauncherView::new(core, show, w, cx)),
                )
                .expect("prototype window");
            #[cfg(feature = "bench-control")]
            cx.spawn(async move |cx| {
                while let Some(command) = receiver.recv().await {
                    // Fixed readiness status includes the core scan state, never raw profile data.
                    let scan = if matches!(command.request["op"].as_str(), Some("status" | "ready"))
                    {
                        let core = control_core.clone();
                        gpui_tokio::Tokio::spawn(cx, async move { core.status().await })
                            .await
                            .ok()
                            .and_then(Result::ok)
                    } else {
                        None
                    };
                    let mut result = window
                        .update(cx, |view, w, cx| view.control(command.request, w, cx))
                        .unwrap_or_else(|e| Err(e.to_string()));
                    if let (Ok(value), Some(scan)) = (&mut result, scan) {
                        value["launcher"] = scan;
                    }
                    let _ = command.reply.send(result);
                }
            })
            .detach();
            if show {
                cx.activate(true);
            }
        });
    Ok(())
}
