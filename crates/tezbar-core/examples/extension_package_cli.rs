//! Run the production package importer without compiling or launching the GUI.
//! Set TEZBAR_DATA_DIR and TEZBAR_CONFIG_DIR to an isolated profile for QA.
fn main() {
    let code = tezbar_core::native_v2_cli::dispatch().unwrap_or_else(|| {
        eprintln!("Use --extensions-v3-import <manifest.json> <package.zip> or --extensions-v2-list --json");
        2
    });
    std::process::exit(code);
}
