use super::*;
use crate::native_core::common::{read_json, write_json_atomic, CorePaths};
use std::{path::Path, sync::atomic::Ordering};

fn context(path: &Path) -> CoreContext {
    CoreContext::headless(CorePaths {
        data_dir: path.into(),
        config_dir: path.join("config"),
        resource_dir: path.join("resources"),
        temp_dir: path.join("tmp"),
        home_dir: path.into(),
    })
}
fn legacy(ctx: &CoreContext) {
    write_json_atomic(
        &ctx.paths
            .data_dir
            .join("extensions/perplexity/package.json"),
        &json!({
            "title":"Legacy", "platforms":["macOS","Windows","Linux"],
            "commands":[{"name":"ask-perplexity"},{"name":"legacy-only"}]
        }),
    )
    .unwrap();
}
async fn commands(core: &ExtensionsCore) -> Value {
    core.handle("core:extensions:commands", &[])
        .await
        .unwrap()
        .unwrap()
}
fn uncached(ctx: &CoreContext) -> Value {
    registry::handle(ctx, "core:extensions:commands", &[]).unwrap()
}
fn work(core: &ExtensionsCore) -> (u64, u64) {
    (
        core.hash_work.calls.load(Ordering::Relaxed),
        core.hash_work.bytes.load(Ordering::Relaxed),
    )
}
async fn equal_source(core: &ExtensionsCore) -> Value {
    let value = commands(core).await;
    assert_eq!(value, uncached(&core.ctx));
    value
}

#[tokio::test]
async fn unchanged_command_list_does_not_hash_package_bytes_again() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    legacy(&ctx);
    let manifest = process_install::test_install(&ctx, "normal");
    assert!(manifest.get("launcherContributions").is_none());
    let core = ExtensionsCore::new(ctx.clone());
    let first = equal_source(&core).await;
    assert_eq!(first.as_array().unwrap().len(), 2);
    assert_eq!(first[0]["extensionId"], "com.tezbar.native.perplexity");
    assert_eq!(first[1]["cmdName"], "legacy-only");
    let before = work(&core);
    assert!(
        before.0 >= 3 && before.1 > 0,
        "real resolve must hash the fixture"
    );
    for _ in 0..3 {
        assert_eq!(commands(&core).await, first);
    }
    let after = work(&core);
    eprintln!("command-list hashes/bytes: first={before:?}, repeated={after:?}");
    assert_eq!(
        after, before,
        "unchanged async command list rehashed package bytes"
    );
    assert!(!core.worker_running().await);
}

#[tokio::test]
async fn external_activation_add_remove_and_identical_cli_import_are_reconciled() {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    legacy(&ctx);
    let core = ExtensionsCore::new(ctx.clone());
    assert!(equal_source(&core)
        .await
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["extensionId"] == "raycast.perplexity"));
    // Same install path used by CLI import; no ExtensionsCore event is sent.
    let mut manifest =
        process_install::test_install_with(&ctx, "normal", |m| m["title"] = json!("Version One"));
    let v1 = equal_source(&core).await;
    let root = process_install::root(&ctx, "com.tezbar.native.perplexity").unwrap();
    let active = read_json(&root.join("active.json")).unwrap();
    process_install::test_install_with(&ctx, "normal", |m| {
        m["version"] = json!("2.0.0");
        m["title"] = json!("Version Two");
    });
    let v2 = equal_source(&core).await;
    assert_ne!(v1, v2);
    write_json_atomic(&root.join("active.json"), &active).unwrap();
    assert_eq!(equal_source(&core).await, v1);

    // Build an actual trusted local ZIP and use the CLI's real async import API.
    let package = process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap();
    let executable = manifest["artifacts"][process_install::target()]["executable"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("extension.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    zip.start_file(
        &executable,
        zip::write::SimpleFileOptions::default().unix_permissions(0o755),
    )
    .unwrap();
    zip.write_all(&std::fs::read(package.executable).unwrap())
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    manifest["artifacts"][process_install::target()]["sha256"] =
        json!(format!("{:x}", Sha256::digest(&bytes)));
    let archive = temp.path().join("fixture.zip");
    let reference = temp.path().join("release.json");
    std::fs::write(&archive, bytes).unwrap();
    write_json_atomic(&reference, &manifest).unwrap();
    registry::handle(
        &ctx,
        "extension:uninstall",
        &[json!("com.tezbar.native.perplexity")],
    )
    .unwrap();
    assert_ne!(equal_source(&core).await, v1);
    super::v3_cli_import(&ctx, reference.clone(), archive.clone())
        .await
        .unwrap();
    assert_eq!(equal_source(&core).await, v1);
    // Reimport identical version under a different activation directory.
    super::v3_cli_import(&ctx, reference, archive)
        .await
        .unwrap();
    assert_eq!(equal_source(&core).await, v1);
    let before = work(&core);
    assert_eq!(commands(&core).await, v1);
    assert_eq!(work(&core), before);
}

#[tokio::test]
async fn legacy_manifest_icons_roots_and_non_dependencies() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    legacy(&ctx);
    process_install::test_install(&ctx, "normal");
    let core = ExtensionsCore::new(ctx.clone());
    let before = equal_source(&core).await;
    // These values are not read by command_values or the dedup/discovery path.
    write_json_atomic(
        &ctx.paths.config_dir.join("config.json"),
        &json!({"aliases":{"ext-perplexity-ask-perplexity":"ask"}}),
    )
    .unwrap();
    write_json_atomic(
        &ctx.paths
            .data_dir
            .join("extension-preferences/com.tezbar.native.perplexity.json"),
        &json!({"commands":{"ask-perplexity":{"key":"new"}}}),
    )
    .unwrap();
    let hashes = work(&core);
    assert_eq!(commands(&core).await, before);
    assert_eq!(work(&core), hashes);
    let path = ctx
        .paths
        .data_dir
        .join("extensions/perplexity/package.json");
    let mut manifest = read_json(&path).unwrap();
    manifest["commands"][1]["title"] = json!("New Legacy Title");
    manifest["commands"][1]["icon"] = json!("nested/command.svg");
    write_json_atomic(&path, &manifest).unwrap();
    let renamed = equal_source(&core).await;
    assert_ne!(renamed, before);
    let icons = ctx
        .paths
        .data_dir
        .join("extensions/perplexity/assets/nested");
    std::fs::create_dir_all(&icons).unwrap();
    std::fs::write(icons.join("command.svg"), b"<svg>one</svg>").unwrap();
    let with_icon = equal_source(&core).await;
    assert_ne!(with_icon, renamed);
    std::fs::write(icons.join("command.svg"), b"<svg>two</svg>").unwrap();
    let changed_icon = equal_source(&core).await;
    assert_ne!(changed_icon, with_icon);
    std::fs::remove_file(icons.join("command.svg")).unwrap();
    assert_eq!(equal_source(&core).await, renamed);
    // Newly created/removed legacy package under another managed root.
    let other = ctx
        .paths
        .data_dir
        .join("extension-registry/packages/another");
    write_json_atomic(
        &other.join("package.json"),
        &json!({"platforms":["macOS","Windows","Linux"],"commands":[{"name":"extra"}]}),
    )
    .unwrap();
    assert_eq!(equal_source(&core).await.as_array().unwrap().len(), 3);
    std::fs::remove_dir_all(other).unwrap();
    assert_eq!(equal_source(&core).await, renamed);
}

#[tokio::test]
async fn cached_display_never_authorizes_execution_and_corruption_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    legacy(&ctx);
    process_install::test_install(&ctx, "normal");
    let core = ExtensionsCore::new(ctx.clone());
    let original = equal_source(&core).await;
    let package = process_install::resolve(&ctx, "com.tezbar.native.perplexity").unwrap();
    let bytes = std::fs::read(&package.executable).unwrap();
    std::fs::write(&package.executable, b"tampered executable").unwrap();
    // Do not ask the listing to invalidate first: execution must check real bytes.
    let execution = core
        .handle(
            "extension:run-command",
            &[json!({"extensionId":"com.tezbar.native.perplexity","commandName":"ask-perplexity"})],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(execution["ok"], false);
    assert!(execution["message"].as_str().unwrap().contains("Tampered"));
    assert!(!core.worker_running().await);
    let damaged = equal_source(&core).await;
    assert!(damaged
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["extensionId"] != "com.tezbar.native.perplexity"));
    std::fs::write(&package.executable, bytes).unwrap();
    assert_eq!(equal_source(&core).await, original);
    // Extra uninventoried files are also relevant to resolve health.
    std::fs::write(package.dir.join("extra.bin"), b"extra").unwrap();
    assert_eq!(equal_source(&core).await, damaged);
    std::fs::remove_file(package.dir.join("extra.bin")).unwrap();
    assert_eq!(equal_source(&core).await, original);
    let manifest = std::fs::read(package.dir.join("extension.json")).unwrap();
    std::fs::write(package.dir.join("extension.json"), b"{}").unwrap();
    assert_eq!(equal_source(&core).await, damaged);
    std::fs::write(package.dir.join("extension.json"), manifest).unwrap();
    assert_eq!(equal_source(&core).await, original);
}

#[tokio::test]
async fn reconciliation_expiry_and_failed_refresh_recover_without_sleeping() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let core = ExtensionsCore::new(ctx.clone());
    let original = equal_source(&core).await;
    let before = work(&core);
    core.commands.expire();
    assert_eq!(commands(&core).await, original);
    assert!(
        work(&core).1 > before.1,
        "maximum age must force actual byte reconciliation"
    );
    let mutation = core.commands.mutation();
    assert!(core
        .handle("core:extensions:commands", &[])
        .await
        .unwrap()
        .is_err());
    drop(mutation);
    assert_eq!(equal_source(&core).await, original);
    let recovered = work(&core);
    assert_eq!(commands(&core).await, original);
    assert_eq!(work(&core), recovered);
}

#[cfg(unix)]
#[tokio::test]
async fn failed_input_probe_is_not_cached_forever() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let core = ExtensionsCore::new(ctx.clone());
    let original = equal_source(&core).await;
    let icon = process_install::root(&ctx, "com.tezbar.native.perplexity")
        .unwrap()
        .join("icon.png");
    std::os::unix::fs::symlink("icon.png", &icon).unwrap();
    assert_eq!(equal_source(&core).await, original);
    let before = work(&core);
    assert_eq!(commands(&core).await, original);
    assert!(
        work(&core).1 > before.1,
        "failed probe must bypass, not poison cache"
    );
    std::fs::remove_file(icon).unwrap();
    assert_eq!(equal_source(&core).await, original);
    let recovered = work(&core);
    assert_eq!(commands(&core).await, original);
    assert_eq!(work(&core), recovered);
}

#[tokio::test]
async fn old_refresh_cannot_publish_after_concurrent_core_uninstall() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    legacy(&ctx);
    process_install::test_install(&ctx, "normal");
    let core = ExtensionsCore::new(ctx.clone());
    let (ready, received) = tokio::sync::oneshot::channel();
    let (release, held) = std::sync::mpsc::channel();
    core.commands.before_publish(move || {
        ready.send(()).unwrap();
        held.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
    });
    let listing_core = core.clone();
    let pending = tokio::spawn(async move { commands(&listing_core).await });
    received.await.unwrap();
    assert_eq!(
        core.handle(
            "extension:uninstall",
            &[json!("com.tezbar.native.perplexity")]
        )
        .await
        .unwrap()
        .unwrap(),
        true
    );
    release.send(()).unwrap();
    let listed = pending.await.unwrap();
    assert_eq!(listed, uncached(&ctx));
    assert!(listed
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["extensionId"] == "raycast.perplexity"));
    assert_eq!(commands(&core).await, listed);
}

#[tokio::test]
async fn uncached_fallback_cannot_return_removed_commands_after_uninstall() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    legacy(&ctx);
    process_install::test_install(&ctx, "normal");
    // A valid but oversized manifest forces the uncached fallback.
    write_json_atomic(
        &ctx.paths.data_dir.join("extensions/oversized/package.json"),
        &json!({"padding":"x".repeat(2 * 1024 * 1024 + 1),"commands":[]}),
    ).unwrap();
    let core = ExtensionsCore::new(ctx.clone());
    let (ready, received) = tokio::sync::oneshot::channel();
    let (release, held) = std::sync::mpsc::channel();
    core.commands.before_publish(move || {
        ready.send(()).unwrap();
        held.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    });
    let reader = core.clone();
    let pending = tokio::spawn(async move { commands(&reader).await });
    received.await.unwrap();
    core.handle("extension:uninstall", &[json!("com.tezbar.native.perplexity")])
        .await.unwrap().unwrap();
    release.send(()).unwrap();
    let listed = pending.await.unwrap();
    assert_eq!(listed, uncached(&ctx));
    assert!(!core.commands.has_snapshot());
}

#[tokio::test]
async fn uncached_fallback_fences_external_activation_and_removal() {
    for remove in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        legacy(&ctx);
        process_install::test_install(&ctx, "normal");
        write_json_atomic(
            &ctx.paths.data_dir.join("extensions/oversized/package.json"),
            &json!({"padding":"x".repeat(2 * 1024 * 1024 + 1),"commands":[]}),
        ).unwrap();
        let core = ExtensionsCore::new(ctx.clone());
        let external = ctx.clone();
        core.commands.before_publish(move || {
            // CLI/filesystem mutation deliberately bypasses CommandList::mutation.
            if remove {
                let root = process_install::root(&external, "com.tezbar.native.perplexity").unwrap();
                std::fs::remove_file(root.join("active.json")).unwrap();
            } else {
                process_install::test_install_with(&external, "normal", |m| {
                    m["version"] = json!("2.0.0");
                    m["commands"][0]["title"] = json!("External fallback update");
                });
            }
        });
        let listed = commands(&core).await;
        assert_eq!(listed, uncached(&ctx));
        assert!(!core.commands.has_snapshot());
        assert!(!core.worker_running().await);
    }
}

#[tokio::test]
async fn old_refresh_cannot_publish_across_external_update() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let core = ExtensionsCore::new(ctx.clone());
    let (ready, received) = tokio::sync::oneshot::channel();
    let (release, held) = std::sync::mpsc::channel();
    core.commands.before_publish(move || {
        ready.send(()).unwrap();
        held.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
    });
    let listing_core = core.clone();
    let pending = tokio::spawn(async move { commands(&listing_core).await });
    received.await.unwrap();
    process_install::test_install_with(&ctx, "normal", |m| {
        m["version"] = json!("2.0.0");
        m["commands"][0]["title"] = json!("Updated during refresh");
    });
    release.send(()).unwrap();
    let listed = pending.await.unwrap();
    assert_eq!(listed, uncached(&ctx));
    assert_eq!(listed[0]["title"], "Updated during refresh");
    assert_eq!(commands(&core).await, listed);
}

#[tokio::test]
async fn cores_are_profile_isolated_and_clones_share_only_their_own_snapshot() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let ctx_a = context(a.path());
    let ctx_b = context(b.path());
    process_install::test_install_with(&ctx_a, "normal", |m| m["title"] = json!("Profile A"));
    process_install::test_install_with(&ctx_b, "normal", |m| m["title"] = json!("Profile B"));
    let core_a = ExtensionsCore::new(ctx_a);
    let core_b = ExtensionsCore::new(ctx_b);
    let value_a = equal_source(&core_a).await;
    let value_b = equal_source(&core_b).await;
    assert_ne!(value_a, value_b);
    let before = work(&core_a);
    assert_eq!(commands(&core_a.clone()).await, value_a);
    assert_eq!(work(&core_a), before);
}

// Environment mutation is confined to a single-test subprocess, so this does
// not race unrelated parallel tests or alter the parent test process's roots.
#[test]
fn environment_roots_and_direct_packages_are_part_of_the_snapshot_key() {
    const MARKER: &str = "TEZBAR_COMMAND_LIST_ENV_TEST";
    if let Some(base) = std::env::var_os(MARKER) {
        let base = std::path::PathBuf::from(base);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let ctx = context(&base.join("profile"));
            let core = ExtensionsCore::new(ctx);
            let first = equal_source(&core).await;
            assert_eq!(first[0]["cmdName"], "first");
            std::env::set_var("SUPERCMD_EXTENSION_PATHS", base.join("second-root"));
            let second = equal_source(&core).await;
            assert_eq!(second[0]["cmdName"], "second");
            assert_ne!(first, second);
            std::env::remove_var("SUPERCMD_EXTENSION_PATHS");
            assert_eq!(equal_source(&core).await, json!([]));
        });
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    for (path, command) in [
        ("first-direct", "first"),
        ("second-root/second-package", "second"),
    ] {
        write_json_atomic(
            &temp.path().join(path).join("package.json"),
            &json!({"platforms":["macOS","Windows","Linux"],"commands":[{"name":command}]}),
        )
        .unwrap();
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "native_core::extensions::command_list_tests::environment_roots_and_direct_packages_are_part_of_the_snapshot_key", "--nocapture"])
        .env(MARKER, temp.path()).env("SUPERCMD_EXTENSION_PATHS", temp.path().join("first-direct"))
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn continuously_invalidated_refresh_fails_boundedly_then_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let core = ExtensionsCore::new(ctx);
    let cache = core.commands.clone();
    core.commands.before_publish(move || {
        cache.clear();
        let next = cache.clone();
        cache.before_publish(move || next.clear());
    });
    assert!(core
        .handle("core:extensions:commands", &[])
        .await
        .unwrap()
        .is_err());
    assert!(!core.commands.has_snapshot());
    assert_eq!(work(&core).0, 6, "refresh must give up after two attempts");
    equal_source(&core).await;
    let recovered = work(&core);
    commands(&core).await;
    assert_eq!(work(&core), recovered);
}

#[tokio::test]
async fn overlapping_requests_share_one_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    process_install::test_install(&ctx, "normal");
    let core = ExtensionsCore::new(ctx);
    let clone = core.clone();
    let (a, b, c) = tokio::join!(commands(&core), commands(&clone), commands(&core));
    assert_eq!(a, b);
    assert_eq!(b, c);
    assert_eq!(
        work(&core).0,
        3,
        "single-flight list refresh must not duplicate verification"
    );
}

#[tokio::test]
async fn worker_install_update_reinstall_and_failure_invalidate_the_core_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    legacy(&ctx);
    process_install::test_install(&ctx, "normal");
    let mut core = ExtensionsCore::new(ctx.clone());
    let path = ctx
        .paths
        .data_dir
        .join("extensions/perplexity/package.json");
    core.worker_override = Some(Arc::new(move |channel, _args| {
        let path = path.clone();
        Box::pin(async move {
            if channel == "extensions:reinstall" {
                return Err("fixture install failed".into());
            }
            let mut manifest = read_json(&path)?;
            manifest["commands"][1]["title"] = json!(channel);
            write_json_atomic(&path, &manifest)?;
            Ok(json!(true))
        })
    }));
    for channel in [
        "extension:install",
        "extensions:install",
        "extensions:reinstall",
    ] {
        commands(&core).await;
        assert!(core.commands.has_snapshot());
        // Unknown package deliberately takes the compatibility worker path;
        // the test override never starts Bun or accesses a remote catalog.
        let result = core
            .handle(channel, &[json!("raycast.command-list-only-fixture")])
            .await
            .unwrap();
        assert_eq!(result.is_err(), channel == "extensions:reinstall");
        assert!(!core.commands.has_snapshot());
        equal_source(&core).await;
    }
}

#[tokio::test]
async fn oversized_inputs_and_outputs_use_uncached_source_without_retention() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let path = ctx.paths.data_dir.join("extensions/large/package.json");
    // Small input but a large expanded descriptor list (extensionTitle and
    // keywords are repeated per command). No oversized snapshot is retained.
    let title = "a".repeat(500_000);
    let manifest = json!({"title":title,"platforms":["macOS","Windows","Linux"],"commands":(0..10).map(|i| json!({"name":format!("command-{i}")})).collect::<Vec<_>>()});
    write_json_atomic(&path, &manifest).unwrap();
    let core = ExtensionsCore::new(ctx.clone());
    let output = equal_source(&core).await;
    assert!(serde_json::to_vec(&output).unwrap().len() > 8 * 1024 * 1024);
    assert!(!core.commands.has_snapshot());
    // Oversize manifests bypass the probe, retaining the existing uncached
    // behavior; repairing them immediately permits a normal cached snapshot.
    write_json_atomic(
        &path,
        &json!({"title":"x".repeat(2 * 1024 * 1024 + 1), "commands":[]}),
    )
    .unwrap();
    commands(&core).await;
    assert!(!core.commands.has_snapshot());
    write_json_atomic(
        &path,
        &json!({"platforms":["macOS","Windows","Linux"],"commands":[{"name":"small"}]}),
    )
    .unwrap();
    assert_eq!(equal_source(&core).await[0]["cmdName"], "small");
    assert!(core.commands.has_snapshot());
}
