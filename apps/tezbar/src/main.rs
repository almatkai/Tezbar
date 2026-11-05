#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use gpui::*;
use std::path::{Path, PathBuf};

fn profile(path: &Path) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        path.is_absolute() && !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
        "absolute non-symlink profile required"
    );
    let root = path.canonicalize()?;
    let temp = std::env::temp_dir().canonicalize()?;
    anyhow::ensure!(
        root.starts_with(&temp) && root != temp,
        "profile must be a child of OS temp"
    );
    anyhow::ensure!(
        !std::fs::symlink_metadata(root.join(".tezbar-native-preview"))?
            .file_type()
            .is_symlink(),
        "symlink marker rejected"
    );
    anyhow::ensure!(
        std::fs::read_to_string(root.join(".tezbar-native-preview"))?
            == "com.tezbar.preview.native\n",
        "marked preview profile required"
    );
    for name in ["data", "config", "home", "temp"] {
        let child = root.join(name);
        if child.exists() {
            anyhow::ensure!(
                !std::fs::symlink_metadata(&child)?.file_type().is_symlink()
                    && child.canonicalize()? == child,
                "symlink in profile"
            );
        }
        std::fs::create_dir_all(child)?;
    }
    Ok(root)
}
struct Empty;
impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
fn main() -> anyhow::Result<()> {
    if let Some(code) = tezbar_core::child_supervisor::dispatch() {
        std::process::exit(code);
    }
    if let Some(code) = tezbar_core::native_v2_cli::dispatch() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        println!(
            "{}",
            serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"source":env!("TEZBAR_NATIVE_GIT_SHA"),"dirty":env!("TEZBAR_NATIVE_DIRTY"),"rustc_commit":env!("TEZBAR_NATIVE_RUSTC"),"preview":true,"gpui_rev":"c84af5b99cbc366d675064e52b23664a015b6e35","runtime_shaders":cfg!(feature="runtime-shaders")})
        );
        return Ok(());
    }
    if args == ["--help"] {
        println!(
            "Tezbar native M0 preview: --profile <marked OS-temp directory>. Opens a hidden empty PopUp; no desktop integration."
        );
        return Ok(());
    }
    anyhow::ensure!(
        args.len() == 2 && args[0] == "--profile",
        "--profile <marked OS-temp directory> required"
    );
    let _profile = profile(Path::new(&args[1]))?;
    gpui_platform::application().run(|cx| {
        cx.open_window(
            WindowOptions {
                kind: WindowKind::PopUp,
                show: false,
                focus: false,
                titlebar: None,
                ..Default::default()
            },
            |_, cx| cx.new(|_| Empty),
        )
        .expect("hidden preview window");
    });
    Ok(())
}
