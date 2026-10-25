use super::*;
use crate::native_core::common::{write_json_atomic, CorePaths};
use std::io::Write;
fn context(path: &std::path::Path) -> CoreContext {
    CoreContext::headless(CorePaths {
        data_dir: path.into(),
        config_dir: path.join("config"),
        resource_dir: path.join("resources"),
        temp_dir: path.join("tmp"),
        home_dir: path.into(),
    })
}
fn manifest() -> Value {
    json!({"schemaVersion":3,"id":"com.tezbar.native.fixture","slug":"fixture","name":"Fixture","version":"1.0.0","runtime":"rust","execution":"process","protocolVersion":1,"platforms":["macOS","Windows","Linux"],"replaces":["raycast.fixture"],"capabilities":["launcher.publish"],"commands":[{"name":"fixture","title":"Fixture","runtime":"rust","arguments":[{"name":"value","type":"text"}],"launcher":{"recommendation":760}}],"launcherContributions":[{"id":"items","commandName":"fixture","argumentName":"value","surfaces":["query","argument"],"selectors":{"terms":["alpha","slow"]},"limit":12,"timeoutMs":1500,"index":true}],"files":{},"artifacts":{}})
}
#[test]
fn bounded_selectors_and_own_command_index_validation() {
    let m = manifest();
    validate(&m).unwrap();
    let d = &m["launcherContributions"][0];
    assert!(selected(d, "ALPHA"));
    assert!(!selected(d, "unrelated"));
    assert!(!selected(d, ""));
    let mut n = d.clone();
    n["selectors"] = json!({"digitRun":{"min":2,"max":5}});
    assert!(selected(&n, "12"));
    assert!(!selected(&n, "123456"));
    let row = json!({"id":"saved:x","title":"雪 café 😀","subtitle":"Saved","score":430,"tokens":["雪 café 😀"],"action":{"type":"run-extension-command","commandName":"fixture","argumentValues":{"value":"saved"}}});
    let normalized = rows(&m, d, &json!([row.clone()]), true).unwrap();
    assert_eq!(normalized[0]["action"]["extensionId"], m["id"]);
    let mut invalid = row.clone();
    invalid["pid"] = json!(12);
    assert!(rows(&m, d, &json!([invalid]), true).is_err());
    let mut invalid = row.clone();
    invalid["action"]["commandName"] = json!("unowned");
    assert!(rows(&m, d, &json!([invalid]), false).is_err());
    let mut invalid = row;
    invalid["action"]["argumentValues"]["unowned"] = json!("x");
    assert!(rows(&m, d, &json!([invalid]), false).is_err());
    let mut invalid = m.clone();
    invalid["launcherContributions"][0]["selectors"] = json!({"regex":".*"});
    assert!(validate(&invalid).is_err());
    let mut invalid = m.clone();
    invalid["launcherContributions"][0]["argumentName"] = json!("unowned");
    assert!(validate(&invalid).is_err());
    let mut invalid = m.clone();
    invalid["launcherContributions"][0]["limit"] = json!(33);
    assert!(validate(&invalid).is_err());
}
#[test]
fn command_metadata_is_validated_with_absent_or_empty_contributions() {
    let mut metadata_only = manifest();
    metadata_only
        .as_object_mut()
        .unwrap()
        .remove("launcherContributions");
    metadata_only["commands"][0]["launcher"]["inlineArgument"] =
        json!({"name":"value","title":"Value","prompt":"Enter value"});
    for contributions in [None, Some(json!([]))] {
        let mut valid = metadata_only.clone();
        if let Some(ds) = contributions {
            valid["launcherContributions"] = ds;
        }
        validate(&valid).unwrap();
        for id in [
            "extcmd:com.tezbar.native.fixture:fixture",
            "extcmd:raycast.fixture:fixture",
        ] {
            let mut own = valid.clone();
            own["commands"][0]["launcher"]["id"] = json!(id);
            validate(&own).unwrap();
        }
        for (field, value) in [
            ("id", json!("extcmd:com.tezbar.native.other:fixture")),
            (
                "inlineArgument",
                json!({"name":"undeclared","title":"Value","prompt":"Enter value"}),
            ),
            ("recommendation", json!(2001)),
            ("recommendation", json!(-1)),
        ] {
            let mut invalid = valid.clone();
            invalid["commands"][0]["launcher"][field] = value;
            assert!(
                validate(&invalid).is_err(),
                "accepted invalid {field}: {invalid}"
            );
        }
        valid["commands"][0]
            .as_object_mut()
            .unwrap()
            .remove("launcher");
        validate(&valid).unwrap(); // Pre-launcher manifests stay compatible.
    }
}
fn install_sdk(ctx: &CoreContext, exe: &std::path::Path) -> Value {
    install_binary(ctx, exe, manifest())
}
fn install_binary(ctx: &CoreContext, exe: &std::path::Path, mut m: Value) -> Value {
    let bytes = std::fs::read(exe).unwrap();
    let digest = format!("{:x}", Sha256::digest(&bytes));
    m["files"] = json!({});
    m["artifacts"] = json!({});
    let name = if cfg!(windows) {
        "bin/fixture.exe"
    } else {
        "bin/fixture"
    };
    m["files"][name] = json!(digest);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("extension.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(serde_json::to_string(&m).unwrap().as_bytes())
        .unwrap();
    zip.start_file(
        name,
        zip::write::SimpleFileOptions::default().unix_permissions(0o755),
    )
    .unwrap();
    zip.write_all(&bytes).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let archive = ctx.paths.temp_dir.join("fixture.zip");
    std::fs::create_dir_all(&ctx.paths.temp_dir).unwrap();
    std::fs::write(&archive, &bytes).unwrap();
    m["artifacts"][process_install::target()] = json!({"url":"https://example.com/fixture.zip","sha256":format!("{:x}",Sha256::digest(&bytes)),"executable":name,"executableSha256":digest});
    process_install::test_import_target(ctx, &m, process_install::target(), &archive).unwrap();
    m
}
#[tokio::test]
async fn actual_sdk_package_launcher_index_fences_and_uninstall() {
    // Run this opt-in fixture test with the separately built, independent SDK
    // fixture executable. CI may provide the same fixture; no runtime dependency.
    let Some(executable) = std::env::var_os("TEZBAR_LAUNCHER_SDK_FIXTURE") else {
        eprintln!("SDK fixture not provided; execute with TEZBAR_LAUNCHER_SDK_FIXTURE for integration evidence");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let m = install_sdk(&ctx, std::path::Path::new(&executable));
    let mut other = manifest();
    other["id"] = json!("com.tezbar.native.other-fixture");
    other["slug"] = json!("other-fixture");
    other["launcherContributions"][0]["selectors"] = json!({"terms":["other-only"]});
    install_binary(&ctx, std::path::Path::new(&executable), other);
    let sessions = super::super::native::NativeSessions::default();
    assert!(sessions
        .test_launcher(&ctx, json!({"text":"unrelated","surface":"query"}))
        .await
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(sessions.test_process_count(), 0);
    assert!(sessions
        .test_launcher(&ctx, json!({"text":"alpha","surface":"query","deep":true}))
        .await
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(sessions.test_process_count(), 0);
    sessions
        .test_launcher(&ctx, json!({"text":"alpha","surface":"query"}))
        .await;
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            if sessions
                .test_launcher(&ctx, json!({"text":"alpha","surface":"query"}))
                .await
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == "fixture:query")
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(sessions.test_process_count(), 1);
    let cached = read_index(&ctx, &m).unwrap();
    assert_eq!(cached["indexes"]["items"]["rows"][0]["title"], "雪 café 😀");
    assert_eq!(
        index_rows(&ctx, &m, &m["launcherContributions"][0], "CAFÉ")
            .unwrap()
            .len(),
        1
    );
    let p = sessions
        .test_process("com.tezbar.native.fixture")
        .await
        .unwrap();
    assert!(p.launcher_enabled());
    assert!(publish(
        &p,
        &json!({"contributionId":"items","revision":1,"rows":[]})
    )
    .is_err());
    sessions
        .test_launcher(&ctx, json!({"text":"slow","surface":"query"}))
        .await;
    sessions
        .test_launcher(&ctx, json!({"text":"alpha new","surface":"query"}))
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    let rows = sessions
        .test_launcher(&ctx, json!({"text":"alpha new","surface":"query"}))
        .await;
    assert!(!rows
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["title"] == "slow"));
    sessions.clear_slug("fixture").await;
    assert!(!p.alive());
    assert!(publish(
        &p,
        &json!({"contributionId":"items","revision":2,"rows":[]})
    )
    .is_err());
    let isolated = tempfile::tempdir().unwrap();
    assert!(read_index(&context(isolated.path()), &m).unwrap()["indexes"].is_null());
    registry::handle(
        &ctx,
        "extension:uninstall",
        &[json!("com.tezbar.native.fixture")],
    )
    .unwrap();
    assert!(!path(&ctx, "com.tezbar.native.fixture").unwrap().exists());
    assert!(sessions
        .test_launcher(&ctx, json!({"text":"雪","surface":"query"}))
        .await
        .as_array()
        .unwrap()
        .is_empty());
    // A changed package fingerprint cannot resurrect an old persisted snapshot.
    let mut changed = m;
    changed["version"] = json!("2.0.0");
    write_json_atomic(&path(&ctx, "com.tezbar.native.fixture").unwrap(), &cached).unwrap();
    assert!(read_index(&ctx, &changed).unwrap()["indexes"].is_null());
    sessions.clear().await;
}

#[tokio::test]
async fn actual_external_package_bootstraps_only_its_declared_index() {
    // Optional independently built package input. The fixture JSON contains its
    // manifest, executable and owned temporary migration data, never user files.
    let Some(input) = std::env::var_os("TEZBAR_LAUNCHER_PACKAGE_FIXTURE") else {
        eprintln!("External launcher package fixture not provided");
        return;
    };
    let fixture: Value = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    for (name, value) in fixture["migrationFiles"].as_object().unwrap() {
        let path = dir.path().join(process_install::relative(name).unwrap());
        write_json_atomic(&path, value).unwrap();
    }
    let m = install_binary(
        &ctx,
        std::path::Path::new(fixture["executable"].as_str().unwrap()),
        fixture["manifest"].clone(),
    );
    let id = m["id"].as_str().unwrap();
    let sessions = super::super::native::NativeSessions::default();
    sessions.start(&ctx).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            if read_index(&ctx, &m).unwrap()["indexes"]
                .as_object()
                .is_some_and(|a| !a.is_empty())
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(sessions.test_process_count(), 1);
    let rows = sessions
        .test_launcher(&ctx, json!({"text":fixture["query"],"surface":"query"}))
        .await;
    let row = rows.as_array().unwrap().first().unwrap();
    assert_eq!(row["action"]["extensionId"], id);
    assert_eq!(
        row["action"]["argumentValues"],
        fixture["expectedArguments"]
    );
    assert!(row["ephemeral"].as_bool().unwrap());
    sessions.clear().await;
    registry::handle(&ctx, "extension:uninstall", &[json!(id)]).unwrap();
    assert!(!path(&ctx, id).unwrap().exists());
}

#[tokio::test]
async fn optional_index_activation_failure_keeps_command_sessions_available() {
    let Some(executable) = std::env::var_os("TEZBAR_LAUNCHER_SDK_FIXTURE") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let mut m = manifest();
    m["launcherContributions"][0]["id"] = json!("different-index");
    install_binary(&ctx, std::path::Path::new(&executable), m);
    let sessions = super::super::native::NativeSessions::default();
    let result = sessions
        .handle(
            &ctx,
            &std::sync::Arc::new(std::sync::Mutex::new(())),
            "extension:run-command",
            &[json!({"extensionId":"com.tezbar.native.fixture","commandName":"fixture"})],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["mode"], "view");
    sessions.clear().await;
}

fn install_query_fixture(ctx: &CoreContext, mode: &str) {
    process_install::test_install_with(ctx, mode, |m| {
        m["capabilities"] = json!(["launcher.publish"]);
        m["commands"][0]["arguments"] = json!([{"name":"value","type":"text"}]);
        m["launcherContributions"] = json!([{"id":"items","commandName":"ask-perplexity","surfaces":["query"],"selectors":{"terms":["alpha"]},"limit":12,"timeoutMs":80,"index":true}]);
    });
}
fn fixture_calls(path: &std::path::Path) -> usize {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .count()
}
async fn wait_for_dynamic(
    sessions: &super::super::native::NativeSessions,
    ctx: &CoreContext,
    query: &Value,
) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            let rows = sessions.test_launcher(ctx, query.clone()).await;
            if rows
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == "dynamic:fixture")
            {
                return rows;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture must recover within deadline")
}
// Reproduce the renderer's invalidation -> 80ms debounce -> same search loop,
// through actual installed executables and both directions of the protocol.
async fn replay_invalidations(
    sessions: &super::super::native::NativeSessions,
    ctx: &CoreContext,
    query: &Value,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<()>,
) -> usize {
    let mut count = 0;
    let until = tokio::time::Instant::now() + std::time::Duration::from_millis(450);
    while matches!(
        tokio::time::timeout_at(until, events.recv()).await,
        Ok(Some(()))
    ) {
        count += 1;
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        sessions.test_launcher(ctx, query.clone()).await;
        if tokio::time::Instant::now() >= until {
            break;
        }
    }
    count
}
async fn failed_query_settles(failure: &str) {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let ctx = context(dir.path()).with_event_sink(std::sync::Arc::new(move |channel, _| {
        if channel == "launcher:invalidated" {
            let _ = tx.send(());
        }
    }));
    install_query_fixture(&ctx, "launcher");
    let sessions = super::super::native::NativeSessions::default();
    let query = json!({"text":"alpha","surface":"query"});
    wait_for_dynamic(&sessions, &ctx, &query).await;
    let p = sessions
        .test_process("com.tezbar.native.perplexity")
        .await
        .unwrap();
    assert!(
        p.launcher_revision
            .load(std::sync::atomic::Ordering::Acquire)
            > 0
    );
    let data = p.host.paths().unwrap().0;
    let calls = data.join("launcher-calls");
    let control = data.join("launcher-failure");
    assert_eq!(fixture_calls(&calls), 1);
    while events.try_recv().is_ok() {}
    std::fs::write(&control, failure).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(510)).await;
    sessions.test_launcher(&ctx, query.clone()).await;
    let invalidations = replay_invalidations(&sessions, &ctx, &query, &mut events).await;
    let count = fixture_calls(&calls);
    let rows = sessions.test_launcher(&ctx, query.clone()).await;
    // Stop/reap even on the red verdict, so no failed test leaks a child.
    if count != 2 {
        sessions.clear().await;
        assert_eq!(
            count, 2,
            "{failure}: failure completion must not self-retry"
        );
    }
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "dynamic:fixture"),
        "{failure}: retain the previous valid query snapshot"
    );
    assert_eq!(
        invalidations, 0,
        "failure must not invalidate the same query"
    );
    // The next demand after backoff can retry; there is no autonomous timer.
    std::fs::remove_file(&control).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert_eq!(
        fixture_calls(&calls),
        2,
        "no autonomous retry after backoff expires"
    );
    wait_for_dynamic(&sessions, &ctx, &query).await;
    // Cached rows may be returned immediately, so also await completion.
    tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fixture_calls(&calls), 3);
    // A new query bypasses the failure backoff.
    std::fs::write(&control, failure).unwrap();
    let other = json!({"text":"alpha other","surface":"query"});
    sessions.test_launcher(&ctx, other.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    assert_eq!(fixture_calls(&calls), 4);
    std::fs::remove_file(&control).unwrap();
    wait_for_dynamic(&sessions, &ctx, &query).await;
    assert_eq!(fixture_calls(&calls), 5);
    // A genuine publication bypasses the same-query failure backoff too.
    std::fs::write(&control, failure).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(510)).await;
    sessions.test_launcher(&ctx, query.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    assert_eq!(fixture_calls(&calls), 6);
    std::fs::remove_file(&control).unwrap();
    p.call(
        "fixture.publish",
        json!({}),
        std::time::Duration::from_secs(2),
    )
    .await
    .unwrap();
    wait_for_dynamic(&sessions, &ctx, &query).await;
    assert_eq!(fixture_calls(&calls), 7);
    sessions.clear().await;
}
#[tokio::test]
async fn failed_launcher_rpc_settles_and_recovers() {
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        failed_query_settles("error"),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn invalid_launcher_rows_settle_and_recover() {
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        failed_query_settles("invalid"),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn timed_out_launcher_query_settles_and_recovers() {
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        failed_query_settles("timeout"),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn launcher_startup_failure_settles_without_self_retry() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let ctx = context(dir.path()).with_event_sink(std::sync::Arc::new(move |channel, _| {
        if channel == "launcher:invalidated" {
            let _ = tx.send(());
        }
    }));
    install_query_fixture(&ctx, "launcher-init-error");
    let sessions = super::super::native::NativeSessions::default();
    let query = json!({"text":"alpha","surface":"query"});
    sessions.test_launcher(&ctx, query.clone()).await;
    let starts = ctx
        .paths
        .data_dir
        .join("external-extensions/com.tezbar.native.perplexity/starts");
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        while fixture_calls(&starts) == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let invalidations = replay_invalidations(&sessions, &ctx, &query, &mut events).await;
    if invalidations != 0 {
        sessions.clear().await;
        assert_eq!(invalidations, 0, "startup failure must not self-invalidate");
    }
    for _ in 0..3 {
        sessions.test_launcher(&ctx, query.clone()).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        fixture_calls(&starts),
        1,
        "startup failures share demand backoff"
    );
    std::fs::write(starts.parent().unwrap().join("allow-initialize"), b"yes").unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert_eq!(
        fixture_calls(&starts),
        1,
        "startup failures have no retry timer"
    );
    wait_for_dynamic(&sessions, &ctx, &query).await;
    assert_eq!(
        fixture_calls(&starts),
        2,
        "later demand can recover startup"
    );
    sessions.clear().await;
}
