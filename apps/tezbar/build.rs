use std::process::Command;
fn main() {
    let workspace = std::fs::canonicalize(
        std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../.."),
    )
    .expect("Native workspace directory");
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
    // Cargo's ordinary dependency rebuild does not rerun this package's build
    // script. Re-evaluate Git provenance for EVERY Cargo invocation, including
    // edits or new files in any workspace member. This sentinel never exists.
    let sentinel = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("provenance-must-be-rechecked");
    println!("cargo:rerun-if-changed={}", sentinel.display());
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
            .arg("--no-optional-locks")
            .args(args)
            .current_dir(&workspace)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    // An ignored archive nested in another repository must not inherit that
    // repository's clean status or HEAD as an attestation of its own contents.
    let own_repository = git(&["rev-parse", "--show-toplevel"])
        .and_then(|root| std::fs::canonicalize(root).ok())
        .is_some_and(|root| root == workspace)
        && git(&["ls-files", "--error-unmatch", "--", "apps/tezbar/build.rs"]).is_some();
    let tracked_notes = std::fs::canonicalize(&notes_path).ok().is_some_and(|path| {
        path.strip_prefix(&workspace)
            .ok()
            .and_then(|relative| relative.to_str())
            .is_some_and(|relative| git(&["ls-files", "--error-unmatch", "--", relative]).is_some())
    });
    let observed_sha = git(&["rev-parse", "HEAD"]);
    let sha = std::env::var("TEZBAR_NATIVE_SOURCE_SHA")
        .ok()
        .or_else(|| observed_sha.clone())
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
        !own_repository
            || !tracked_notes
            || git(&["status", "--porcelain"])
                .map(|v| !v.is_empty())
                .unwrap_or(true)
            || observed_sha.as_deref() != Some(sha.as_str())
    );
}
