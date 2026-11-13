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
    let rust_version = compiler
        .lines()
        .find_map(|line| line.strip_prefix("release: "))
        .expect("compiler release");
    let quoted = |line: &str, key: &str| -> Option<String> {
        line.trim()
            .strip_prefix(key)?
            .trim_start()
            .strip_prefix('=')?
            .trim_start()
            .strip_prefix('"')?
            .split_once('"')
            .map(|(value, _)| value.to_owned())
    };
    let rust_policy = std::fs::read_to_string(workspace.join("rust-toolchain.toml"))
        .ok()
        .and_then(|s| s.lines().find_map(|line| quoted(line, "channel")))
        .unwrap_or_else(|| "unavailable".into());
    let gpui_policy = std::fs::read_to_string(workspace.join("apps/tezbar/Cargo.toml"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|line| line.trim().starts_with("gpui = {"))
                .and_then(|line| {
                    line.split_once("rev = \"")
                        .and_then(|(_, tail)| tail.split_once('"'))
                        .map(|(rev, _)| rev.to_owned())
                })
        })
        .unwrap_or_else(|| "unavailable".into());
    let gpui_sources: Vec<_> = std::fs::read_to_string(workspace.join("Cargo.lock"))
        .unwrap_or_default()
        .split("[[package]]")
        .filter(|block| {
            block
                .lines()
                .any(|line| quoted(line, "name").as_deref() == Some("gpui"))
        })
        .filter_map(|block| block.lines().find_map(|line| quoted(line, "source")))
        .collect();
    let gpui_rev = gpui_sources
        .first()
        .filter(|_| gpui_sources.len() == 1)
        .and_then(|source| source.strip_prefix("git+https://github.com/zed-industries/zed?"))
        .and_then(|s| s.rsplit_once('#'))
        .map(|(_, rev)| rev)
        .filter(|rev| rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or("unavailable");
    let identity_attested =
        rust_version == rust_policy && gpui_rev == gpui_policy && gpui_rev.len() == 40;
    println!("cargo:rustc-env=TEZBAR_NATIVE_IDENTITY_ATTESTED={identity_attested}");
    println!("cargo:rustc-env=TEZBAR_NATIVE_RUST_VERSION={rust_version}");
    println!("cargo:rustc-env=TEZBAR_NATIVE_GPUI_REV={gpui_rev}");
    println!("cargo:rustc-env=TEZBAR_NATIVE_RUST_POLICY_VERSION={rust_policy}");
    println!("cargo:rustc-env=TEZBAR_NATIVE_GPUI_POLICY_REV={gpui_policy}");
    for path in [
        "rust-toolchain.toml",
        "Cargo.lock",
        "apps/tezbar/Cargo.toml",
    ] {
        println!("cargo:rerun-if-changed={}", workspace.join(path).display());
    }
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
    let source_dirty = !own_repository
        || !tracked_notes
        || git(&["status", "--porcelain"])
            .map(|v| !v.is_empty())
            .unwrap_or(true)
        || observed_sha.as_deref() != Some(sha.as_str());
    println!("cargo:rustc-env=TEZBAR_NATIVE_SOURCE_DIRTY={source_dirty}");
    println!(
        "cargo:rustc-env=TEZBAR_NATIVE_DIRTY={}",
        source_dirty || !identity_attested
    );
}
