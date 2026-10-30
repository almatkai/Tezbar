fn main() {
    verify_embedding_payload();
    println!("cargo:rerun-if-env-changed=TAURI_CONFIG");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    let mut config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("tauri.conf.json").expect("Tauri configuration is required"),
    )
    .expect("valid Tauri configuration");
    if let Ok(overrides) = std::env::var("TAURI_CONFIG") {
        let overrides: serde_json::Value =
            serde_json::from_str(&overrides).expect("valid Tauri overrides");
        for key in ["version", "identifier", "productName"] {
            if let Some(value) = overrides.get(key) {
                config[key] = value.clone();
            }
        }
    }
    let version = semver::Version::parse(config["version"].as_str().expect("release version"))
        .expect("valid release version");
    let identifier = config["identifier"]
        .as_str()
        .expect("application identifier");
    release_channels::default_channel(identifier, &version)
        .expect("release version and app identity must agree");
    assert_eq!(
        config["productName"].as_str(),
        Some("Tezbar"),
        "Installer product name must match its release identity"
    );
    println!("cargo:rerun-if-changed=../dist-backend/extension-worker.js");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!(
            r#"cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"#
        );
    }
    tauri_build::build()
}

fn verify_embedding_payload() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
        || std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("aarch64")
    {
        return;
    }
    use sha2::{Digest, Sha256};
    let catalog = "runtime/embedding-macos-arm64.json";
    let payload = "../build/embedding-runtime/resources/embedding-runtime.zip";
    println!("cargo:rerun-if-changed={catalog}");
    println!("cargo:rerun-if-changed={payload}");
    let pin: serde_json::Value =
        serde_json::from_slice(&std::fs::read(catalog).expect("runtime catalog"))
            .expect("runtime catalog JSON");
    let bytes = std::fs::read(payload).expect("Missing pinned embedding payload; see docs/EMBEDDING_RUNTIME.md. Build/stage the reviewed package before compiling macOS arm64.");
    assert_eq!(
        Some(bytes.len() as u64),
        pin["archiveBytes"].as_u64(),
        "embedding payload length"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        pin["archiveSha256"].as_str().unwrap(),
        "embedding payload must match the reviewed catalog"
    );
}

#[allow(dead_code)]
#[path = "src/release_channels.rs"]
mod release_channels;
