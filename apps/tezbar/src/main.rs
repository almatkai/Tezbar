#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() -> anyhow::Result<()> {
    if let Some(code) = tezbar_core::child_supervisor::dispatch() {
        std::process::exit(code);
    }
    if let Some(code) = tezbar_core::native_v2_cli::dispatch() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| anyhow::anyhow!("Native preview arguments must be Unicode"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    if args == ["--version"] {
        println!(
            "{}",
            serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"source":env!("TEZBAR_NATIVE_GIT_SHA"),"dirty":env!("TEZBAR_NATIVE_DIRTY"),"rustc_commit":env!("TEZBAR_NATIVE_RUSTC"),"preview":true,"gpui_rev":"c84af5b99cbc366d675064e52b23664a015b6e35","runtime_shaders":cfg!(feature="runtime-shaders"),"bench_control":cfg!(feature="bench-control")})
        );
        return Ok(());
    }
    if args == ["--help"] {
        println!(
            "Tezbar native preview: --profile <marked OS-temp directory> --resources <matching Resources> [--show] [--launcher-shortcut <accelerator>] [--allow-desktop-integration] [--theme system|light|dark]. --launcher-shortcut enables only that key and the tray; stop the other launcher before claiming its key. Desktop integration alone uses Alt+F19."
        );
        return Ok(());
    }
    if args.is_empty() {
        return tezbar_native::app::run_production(
            tezbar_native::app::production_launch::Admission::current()
                .map_err(anyhow::Error::msg)?,
        );
    }
    tezbar_native::app::run(tezbar_native::cli::Preview::parse(&args)?)
}
