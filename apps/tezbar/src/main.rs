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
        let build = tezbar_native::app::production_launch::current_build_metadata();
        println!(
            "{}",
            serde_json::json!({"version":build["version"],"source":build["sourceCommit"],"dirty":build["dirty"].as_bool().expect("embedded dirty flag").to_string(),"rustc_commit":env!("TEZBAR_NATIVE_RUSTC"),"preview":true,"gpui_rev":build["gpuiRev"],"runtime_shaders":build["runtimeShaders"],"bench_control":build["benchControl"]})
        );
        return Ok(());
    }
    if args == ["--help"] {
        println!(
            "Tezbar native preview: --profile <marked OS-temp directory> --resources <matching Resources> [--show] [--launcher-shortcut <accelerator>] [--allow-desktop-integration] [--theme system|light|dark]. --launcher-shortcut enables only that key and the tray; stop the other launcher before claiming its key. Desktop integration alone uses Alt+F19."
        );
        return Ok(());
    }
    if args == ["--verify-installation"] {
        tezbar_native::app::production_launch::Admission::current().map_err(anyhow::Error::msg)?;
        println!("Production installation verified");
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
