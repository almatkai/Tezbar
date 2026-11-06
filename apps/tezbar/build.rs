use std::process::Command;
fn main() {
    println!("cargo:rerun-if-env-changed=TEZBAR_RELEASE_NOTES_PATH");
    let notes_path = std::env::var_os("TEZBAR_RELEASE_NOTES_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
                .join("../../docs/releases")
                .join(format!(
                    "{}.md",
                    std::env::var("CARGO_PKG_VERSION").unwrap()
                ))
        });
    println!("cargo:rerun-if-changed={}", notes_path.display());
    let notes = std::fs::read_to_string(&notes_path)
        .expect("Native builds require UTF-8 release notes for their exact version");
    assert!(
        !notes.trim().is_empty() && notes.len() <= 1024 * 1024,
        "Release notes must be non-empty and at most 1 MiB"
    );
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("release-notes.md"),
        notes,
    )
    .expect("Stage bundled release notes");
    let compiler = Command::new(std::env::var_os("RUSTC").expect("Cargo RUSTC"))
        .arg("-vV")
        .output()
        .expect("compiler identity");
    assert!(compiler.status.success());
    let compiler = String::from_utf8(compiler.stdout).expect("UTF-8 rustc version");
    let commit = compiler
        .lines()
        .find_map(|line| line.strip_prefix("commit-hash: "))
        .expect("compiler commit");
    println!("cargo:rustc-env=TEZBAR_NATIVE_RUSTC={commit}");
    println!("cargo:rerun-if-env-changed=TEZBAR_NATIVE_SOURCE_SHA");
    for path in [
        "src",
        "Cargo.toml",
        "Cargo.lock",
        "../../crates/tezbar-core/src",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let sha = std::env::var("TEZBAR_NATIVE_SOURCE_SHA")
        .ok()
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .expect("Git SHA or TEZBAR_NATIVE_SOURCE_SHA required");
    assert!(
        sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "40-hex source SHA required"
    );
    if let Some(path) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={path}");
    }
    if let Some(branch) = git(&["symbolic-ref", "HEAD"]) {
        if let Some(path) = git(&["rev-parse", "--git-path", &branch]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    println!("cargo:rustc-env=TEZBAR_NATIVE_GIT_SHA={sha}");
    println!(
        "cargo:rustc-env=TEZBAR_NATIVE_DIRTY={}",
        git(&["status", "--porcelain"])
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    );
}
