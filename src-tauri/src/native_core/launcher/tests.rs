use super::*;
use crate::native_core::common::{read_json, write_json_atomic, CorePaths};
use std::sync::atomic::AtomicUsize;
fn profile(root: &std::path::Path) -> CoreContext {
    std::fs::create_dir_all(root.join("home")).unwrap();
    CoreContext::headless(CorePaths {
        data_dir: root.join("data"),
        config_dir: root.join("config"),
        resource_dir: root.join("resources"),
        temp_dir: root.join("temp"),
        home_dir: root.join("home"),
    })
}
#[tokio::test]
async fn crowded_index_preserves_application_and_keyboard_layout_parity() {
    let dir = tempfile::tempdir().unwrap();
    let core = LauncherCore::new(profile(dir.path()));
    let cases = [
        ("Google Chrome", vec!["chrome", "сркщьу", "хром"]),
        ("Telegram", vec!["telegram", "еудупкфь", "телеграм"]),
        ("Discord", vec!["discord", "вшысщкв", "дискорд"]),
        ("Docker", vec!["docker", "вщслук", "докер"]),
        ("Safari", vec!["safari", "ыфафкш", "сафари"]),
        ("Xcode", vec!["xcode", "чсщву", "икскод"]),
        (
            "Visual Studio Code",
            vec!["vs code", "мы сщву", "vs сщву", "мы code", "вс код"],
        ),
    ];
    let mut docs: Vec<_> = cases
        .iter()
        .map(|(name, _)| {
            stores::document(
                &format!("app:{name}"),
                "applications",
                name,
                &format!("/Applications/{name}.app"),
                &format!("{name} /Applications/{name}.app"),
                json!({"type":"open-app","appName":name}),
                1,
            )
        })
        .collect();
    for n in 0..2500 {
        docs.push(stores::document(
            &format!("file:{n}"),
            "files",
            "vs code",
            &format!("/projects/{n}/vs/code/settings.json"),
            "vs code settings",
            json!({"type":"open-file","path":format!("/projects/{n}/vs/code/settings.json")}),
            now_ms(),
        ));
    }
    core.inner.db.upsert(docs, vec![]).await.unwrap();
    for (name, queries) in cases {
        for query in queries {
            let rows = core.search(query).await.unwrap();
            assert_eq!(
                rows[0]["id"],
                format!("app:{name}"),
                "query={query}, rows={rows}"
            );
            assert!(
                !rows
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"].as_str().unwrap().starts_with("deep-search:")),
                "query={query}"
            );
        }
    }
    let rows = core.search("несуществующее приложение").await.unwrap();
    assert!(rows[0]["id"].as_str().unwrap().starts_with("deep-search:"));
    core.shutdown().await.unwrap();
}
#[tokio::test]
async fn action_dispatch_is_in_process_and_query_learning_is_specific() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let calls = Arc::new(Mutex::new(vec![]));
    let sink = calls.clone();
    ctx.set_router(Arc::new(move |c, a| {
        let sink = sink.clone();
        Box::pin(async move {
            sink.lock().unwrap().push((c, a));
            Ok(json!({"ok":true,"message":"Opened"}))
        })
    }))
    .unwrap();
    let core = LauncherCore::new(ctx);
    core.inner
        .db
        .upsert(
            vec![
                stores::document(
                    "app:gemini",
                    "applications",
                    "Gemini",
                    "Gemini app",
                    "gemini",
                    json!({"type":"open-app","appName":"Gemini"}),
                    1,
                ),
                stores::document(
                    "native:library",
                    "native-command",
                    "Open ~/Library",
                    "Reveal the hidden Library folder in Finder",
                    "in",
                    json!({"type":"run-native-command","commandId":"open-library"}),
                    1,
                ),
            ],
            vec![],
        )
        .await
        .unwrap();
    for _ in 0..3 {
        core.dispatch("search:execute",&[json!({"action":{"type":"run-native-command","commandId":"open-library"},"context":{"resultId":"native:library","query":"library","rank":1}})]).await.unwrap();
    }
    assert_eq!(core.search("").await.unwrap()[0]["id"], "native:library");
    assert_eq!(core.search("gemini").await.unwrap()[0]["id"], "app:gemini");
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .all(|(c, _)| c == "core:native:execute"));
    core.dispatch("search:record-usage",&[json!({"action":{"type":"run-native-command","commandId":"open-library"},"context":{"resultId":"native:library","query":"gemini","rank":2}})]).await.unwrap();
    assert_eq!(
        core.search("gemini").await.unwrap()[0]["id"],
        "native:library"
    );
    let no = core
        .dispatch(
            "search:execute",
            &[json!({"type":"run-shell","command":"echo x; rm -rf /"})],
        )
        .await
        .unwrap();
    assert_eq!(no["ok"], false);
    assert_eq!(calls.lock().unwrap().len(), 3);
    assert!(core.handle("unowned", &[]).await.is_none());
    assert!(core
        .dispatch(
            "search:record-usage",
            &[json!({"type":"open-app","appName":"x"})]
        )
        .await
        .is_err());
    core.shutdown().await.unwrap();
}
#[tokio::test]
async fn settings_shortcut_failure_is_atomic_and_toggles_preserve_concurrent_keys() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    ctx.config
        .patch(&json!({"future":{"keep":true},"commandHotkeys":{"cmd":"old"}}))
        .unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let captured = count.clone();
    ctx.set_router(Arc::new(move |c, _| {
        let captured = captured.clone();
        Box::pin(async move {
            match c.as_str() {
                "core:shortcut:update-command" => {
                    captured.fetch_add(1, Ordering::SeqCst);
                    Ok(json!({"ok":false,"error":"In use"}))
                }
                "native-commands:list" | "core:extensions:commands" => Ok(json!([])),
                _ => Err(format!("Unexpected {c}")),
            }
        })
    }))
    .unwrap();
    let core = LauncherCore::new(ctx.clone());
    let result = core
        .dispatch("update-command-hotkey", &[json!("cmd"), json!("new")])
        .await
        .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(ctx.config.read().unwrap()["commandHotkeys"]["cmd"], "old");
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let mut tasks = vec![];
    for n in 0..8 {
        let c = core.clone();
        tasks.push(tokio::spawn(async move {
            c.dispatch(
                "toggle-command-enabled",
                &[json!(format!("cmd-{n}")), json!(false)],
            )
            .await
            .unwrap()
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    let config = ctx.config.read().unwrap();
    assert_eq!(config["disabledCommands"].as_object().unwrap().len(), 8);
    assert_eq!(config["future"]["keep"], true);
    core.dispatch(
        "save-settings",
        &[json!({"commandAliases":{"cmd":"Қазақ"}})],
    )
    .await
    .unwrap();
    assert_eq!(
        core.dispatch("get-settings", &[]).await.unwrap()["commandAliases"]["cmd"],
        "Қазақ"
    );
    core.shutdown().await.unwrap();
}
#[tokio::test]
async fn local_stores_keep_legacy_ids_and_never_index_clipboard_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let search = ctx.paths.data_dir.join("search");
    std::fs::create_dir_all(&search).unwrap();
    write_json_atomic(&search.join("clipboard.json"),&json!({"items":[{"id":"text:old","kind":"text","text":"password=private-value-do-not-index","createdAt":1,"pinned":true,"isSecret":true}]})).unwrap();
    write_json_atomic(&search.join("notes.json"),&json!({"notes":[{"text":"<h1>Қазақ</h1><p>**Жазба** &amp; [link](https://example.com)</p>","createdAt":42}],"future":123})).unwrap();
    write_json_atomic(&search.join("snippets.json"),&json!({"snippets":[{"id":"snippet:today","trigger":"today","body":"Today ${date}","createdAt":1},{"id":"snippet:user:old","trigger":"greet","label":"Сәлем","body":"Hello ${year}","scope":"work","createdAt":2}],"future":true})).unwrap();
    let core = LauncherCore::new(ctx.clone());
    core.local_refresh().await.unwrap();
    assert_eq!(core.search("Қазақ").await.unwrap()[0]["id"], "note:42");
    let rows = core.dispatch("search:candidates", &[]).await.unwrap();
    assert!(!rows
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["category"] == "clipboard"));
    let snippets = core.dispatch("snippets:list", &[]).await.unwrap();
    let today = snippets
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "snippet:today")
        .unwrap();
    assert_eq!(today["title"], "Get today's date");
    assert_eq!(today["readonly"], true);
    let no = core
        .dispatch("snippets:delete", &[json!("snippet:today")])
        .await
        .unwrap();
    assert_eq!(no["ok"], false);
    let add = core
        .dispatch(
            "snippets:add",
            &[json!({"label":"Test","trigger":"GREET","body":"body"})],
        )
        .await
        .unwrap();
    assert_eq!(add["ok"], false);
    assert_eq!(
        read_json(&search.join("notes.json")).unwrap()["future"],
        123
    );
    assert_eq!(
        read_json(&search.join("snippets.json")).unwrap()["future"],
        true
    );
    core.shutdown().await.unwrap();
}
#[tokio::test]
async fn file_scan_is_bounded_to_profile_roots_and_skip_directories() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let root = ctx.paths.home_dir.join("Desktop");
    std::fs::create_dir_all(root.join("node_modules")).unwrap();
    std::fs::write(root.join("good.rs"), "fn main() {}").unwrap();
    std::fs::write(root.join("node_modules/hidden.ts"), "private").unwrap();
    std::fs::write(ctx.paths.home_dir.join("outside.txt"), "outside").unwrap();
    let db = db::Database::new(ctx.paths.data_dir.join("search/index.sqlite3"));
    providers::scan(ctx.clone(), db.clone(), Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    let rows = db
        .call(|c| {
            let rows: i64 = c
                .query_row("SELECT count(*) FROM documents", [], |r| r.get(0))
                .unwrap();
            Ok(json!(rows))
        })
        .await
        .unwrap();
    assert_eq!(rows, 1);
    let rows = db.search("good".into(), false).await.unwrap();
    assert_eq!(rows[0]["category"], "files");
    assert_eq!(
        providers::resolve(&ctx.paths.home_dir, "/Desktop/..."),
        root.join("")
    );
}
#[test]
fn extension_descriptor_contract_and_surface_catalog() {
    let docs = providers::extension_documents(
        &[
            json!({"id":"ext-sample-main","extName":"sample","cmdName":"main","title":"Launch Sample","extensionTitle":"Sample Extension","description":"test","keywords":["keyword"],"commandArgumentDefinitions":[{"name":"query","required":true}]}),
        ],
        &json!({"commandAliases":{"extcmd:raycast.sample:main":"alias"}}),
    );
    assert_eq!(docs[0]["id"], "extcmd:raycast.sample:main");
    assert_eq!(
        docs[0]["action"]["commandArgumentDefinitions"][0]["name"],
        "query"
    );
    assert_eq!(docs[0]["subtitle"], "Sample Extension");
    let docs = providers::extension_documents(
        &[json!({"extName":"sample","cmdName":"main","title":"Launch Sample"})],
        &json!({"disabledCommands":{"extcmd:raycast.sample:main":true}}),
    );
    assert!(docs.is_empty());
    let docs = providers::command_documents(&[]);
    assert!(docs.iter().any(|d| d["id"] == "command:check-for-updates"
        && d["tokens"].as_str().unwrap().contains("обновление")));
}

#[tokio::test]
async fn startup_failure_can_retry_and_concurrent_starts_spawn_one_loop() {
    let directory = tempfile::tempdir().unwrap();
    let ctx = profile(directory.path());
    std::fs::create_dir_all(&ctx.paths.config_dir).unwrap();
    std::fs::write(ctx.config.path(), b"{invalid").unwrap();
    ctx.set_router(Arc::new(|c, _| {
        Box::pin(async move {
            match c.as_str() {
                "native-commands:list" | "core:extensions:commands" => Ok(json!([])),
                _ => Err(format!("unexpected {c}")),
            }
        })
    }))
    .unwrap();
    let core = LauncherCore::new(ctx.clone());
    assert!(core.start().await.is_err());
    assert!(!core.inner.started.load(Ordering::Acquire));
    assert!(core.inner.tasks.lock().unwrap().is_empty());
    std::fs::write(ctx.config.path(), b"{\"future\":42}").unwrap();
    let mut tasks = vec![];
    for _ in 0..8 {
        let c = core.clone();
        tasks.push(tokio::spawn(async move { c.start().await }));
    }
    for t in tasks {
        t.await.unwrap().unwrap();
    }
    assert!(core.inner.started.load(Ordering::Acquire));
    assert_eq!(core.inner.tasks.lock().unwrap().len(), 1);
    assert_eq!(ctx.config.read().unwrap()["future"], 42);
    core.shutdown().await.unwrap();
    core.shutdown().await.unwrap();
    assert!(core.start().await.is_err());
}
#[tokio::test]
async fn startup_database_failure_can_retry_after_directory_repair() {
    let directory = tempfile::tempdir().unwrap();
    let ctx = profile(directory.path());
    std::fs::write(&ctx.paths.data_dir, b"not a directory").unwrap();
    let core = LauncherCore::new(ctx.clone());
    assert!(core.start().await.is_err());
    assert!(!core.inner.started.load(Ordering::Acquire));
    std::fs::remove_file(&ctx.paths.data_dir).unwrap();
    std::fs::create_dir_all(&ctx.paths.data_dir).unwrap();
    core.start().await.unwrap();
    assert!(core.inner.started.load(Ordering::Acquire));
    core.shutdown().await.unwrap();
}

#[test]
fn path_completion_retains_values_ids_and_open_with_learning() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let desktop = ctx.paths.home_dir.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let apps = json!([{"name":"Visual Studio Code","path":"/Applications/Visual Studio Code.app"},{"name":"TextEdit","path":"/System/Applications/TextEdit.app"}]);
    let apps = apps.as_array().unwrap();
    let docs = providers::app_documents(apps);
    assert_eq!(docs[0]["id"], "app:/Applications/Visual Studio Code.app");
    let grid = providers::complete(&ctx, "`vsc", apps).unwrap();
    assert_eq!(
        grid[0]["id"],
        "path-installed-app:/Applications/Visual Studio Code.app"
    );
    assert_eq!(grid[0]["value"], "/Applications/Visual Studio Code.app");
    assert_eq!(grid[0]["badge"], "Application");
    write_json_atomic(&ctx.paths.data_dir.join("open-with-usage.json"),&json!({"version":1,"keys":{format!("folder:{}",desktop.display()):{"TextEdit":{"count":5,"lastUsedAt":now_ms()}}},"aliases":{"editor":{"Visual Studio Code":{"count":2,"lastUsedAt":now_ms()}}}})).unwrap();
    let rows = providers::complete(&ctx, "/Desktop editor", apps).unwrap();
    assert_eq!(rows[0]["appName"], "Visual Studio Code");
    assert_eq!(
        rows[0]["id"],
        "path-app:recommended:/Applications/Visual Studio Code.app"
    );
    assert_eq!(rows[0]["value"], "/Desktop Visual Studio Code");
    assert_eq!(rows[0]["applicationAction"], "open-with");
    let rows = providers::complete(&ctx, "/Desktop ", apps).unwrap();
    assert_eq!(rows[0]["appName"], "TextEdit");
    assert_eq!(
        rows[1]["title"],
        if cfg!(windows) {
            "Open in File Explorer"
        } else {
            "Open in Finder"
        }
    );
}
#[test]
fn directory_visits_collapse_siblings_and_prefer_strong_concrete_branch() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let code = home.join("code");
    for child in ["aml", "tezbar", "site"] {
        std::fs::create_dir_all(code.join(child)).unwrap();
    }
    std::fs::create_dir_all(home.join("Documents")).unwrap();
    let visits = json!({code.join("aml").to_string_lossy():{"count":4,"lastVisitedAt":now_ms()},code.join("tezbar").to_string_lossy():{"count":7,"lastVisitedAt":now_ms()},code.join("site").to_string_lossy():{"count":2,"lastVisitedAt":now_ms()},home.join("Documents").to_string_lossy():{"count":5,"lastVisitedAt":now_ms()}});
    let ranked = providers::recommendations(&visits, &home);
    assert_eq!(ranked[0].0, code.to_string_lossy());
    assert_eq!(ranked.len(), 2);
    assert_eq!(text::intent("/command")["type"], "extension-command");
    assert_eq!(text::intent("open folder")["type"], "file");
    assert_eq!(text::intent("please explain?")["type"], "ai");
}
#[tokio::test]
async fn concurrent_usage_updates_and_failed_queries_preserve_statistics() {
    let dir = tempfile::tempdir().unwrap();
    let db = db::Database::new(dir.path().join("index.sqlite3"));
    let mut tasks = vec![];
    for _ in 0..4 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..12 {
                db.record(
                    "app:test".into(),
                    json!({"query":"  TEST query  ","rank":2}),
                    true,
                )
                .await
                .unwrap();
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    db.record("app:test".into(), json!({"query":"failed","rank":3}), false)
        .await
        .unwrap();
    let stats=db.call(|c|{let(f,s,total):(u64,u64,u64)=c.query_row("SELECT frequency,success_count,total_count FROM action_stats WHERE action_id='app:test'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();let failed:u64=c.query_row("SELECT count(*) FROM query_action_stats WHERE query='failed'",[],|r|r.get(0)).unwrap();let query:u64=c.query_row("SELECT frequency FROM query_action_stats WHERE query='test query'",[],|r|r.get(0)).unwrap();Ok(json!([f,s,total,failed,query]))}).await.unwrap();
    assert_eq!(stats, json!([49, 48, 49, 0, 48]));
    db.shutdown().await.unwrap();
}
#[tokio::test]
async fn storage_breakdown_uses_only_profile_and_skips_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    std::fs::create_dir_all(ctx.paths.data_dir.join("search/clipboard-images")).unwrap();
    std::fs::write(
        ctx.paths.data_dir.join("search/clipboard-images/image.png"),
        vec![0; 123],
    )
    .unwrap();
    std::fs::write(
        ctx.paths.data_dir.join("search/clipboard.json"),
        vec![0; 55],
    )
    .unwrap();
    #[cfg(unix)]
    {
        let external = dir.path().join("external.bin");
        std::fs::write(&external, vec![0; 2000]).unwrap();
        std::os::unix::fs::symlink(
            external,
            ctx.paths.data_dir.join("search/clipboard-images/link.bin"),
        )
        .unwrap();
    }
    let result = storage::breakdown(ctx).await.unwrap();
    assert_eq!(result["totalBytes"], 178);
    assert_eq!(result["items"][0]["bytes"], 123);
}

#[tokio::test]
async fn deep_search_merges_knowledge_metadata_and_content_without_duplicate_paths() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let count = Arc::new(AtomicUsize::new(0));
    let captured = count.clone();
    ctx.set_router(Arc::new(move|channel,args|{let count=captured.clone();Box::pin(async move{count.fetch_add(1,Ordering::SeqCst);assert_eq!(args[0],"invoice");match channel.as_str(){"core:knowledge:search"=>Ok(json!([{"chunkId":"chunk","sourceId":"source","title":"Invoice","path":"/external/invoice.pdf","text":"Payment details","pageNumber":2,"score":0.9}])),"core:knowledge:search-metadata"=>Ok(json!([{"path":"/external/invoice.pdf","title":"Invoice","score":1.},{"path":"/external/invoice-index.txt","title":"Invoice index","score":0.8}])),_=>Err(format!("unexpected {channel}"))}})})).unwrap();
    let core = LauncherCore::new(ctx);
    let rows = core.search("invoice").await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert!(!rows.as_array().unwrap().is_empty());
    let rows = core.search("!invoice").await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert!(rows
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == "knowledge:chunk"));
    assert!(!rows
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == "file:/external/invoice.pdf"));
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn install_safety_denial_preview_and_audit_are_not_duplicated() {
    for (mode, accepted, dry, installs, records) in [
        ("denied", false, false, 0, 0),
        ("dry", true, true, 0, 0),
        ("allowed", true, false, 1, 1),
        ("failed", true, false, 1, 1),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let ctx = profile(dir.path());
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let capture = calls.clone();
        ctx.set_router(Arc::new(move |channel, args| {
            let capture = capture.clone();
            Box::pin(async move {
                capture.lock().unwrap().push(channel.clone());
                match channel.as_str() {
                    "core:safety:authorize" => {
                        assert_eq!(args[0], "extension.install");
                        assert_eq!(args[1]["extensionId"], "raycast.sample");
                        Ok(json!({"accepted":accepted,"dryRun":dry}))
                    }
                    "extension:install" => {
                        if mode == "failed" {
                            Err("install failed".into())
                        } else {
                            Ok(json!({"id":"raycast.sample"}))
                        }
                    }
                    "core:safety:record" => {
                        assert_eq!(args[0], "extension.install");
                        assert_eq!(args[2]["ok"], mode != "failed");
                        Ok(Value::Null)
                    }
                    _ => Err(format!("Unexpected safety route {channel}")),
                }
            })
        }))
        .unwrap();
        let core = LauncherCore::new(ctx);
        let result = core
            .dispatch(
                "search:execute",
                &[json!({"type":"install-extension","extensionId":"raycast.sample"})],
            )
            .await
            .unwrap();
        assert_eq!(result["ok"], accepted && mode != "failed");
        let calls = calls.lock().unwrap().clone();
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.as_str() == "core:safety:authorize")
                .count(),
            1
        );
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.as_str() == "extension:install")
                .count(),
            installs
        );
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.as_str() == "core:safety:record")
                .count(),
            records
        );
        core.shutdown().await.unwrap();
    }
}

#[test]
fn windows_apps_folder_preserves_launch_targets_and_classic_shortcut_priority() {
    let mut apps = vec![json!({"name":"Classic","path":"C:\\Start Menu\\Classic.lnk"})];
    providers::merge_windows_start_apps(&mut apps,r#"[{"Name":"classic","AppID":"duplicate"},{"Name":" Calculator ","AppID":" Microsoft.Calculator!App "},{"Name":"","AppID":"invalid"}]"#).unwrap();
    assert_eq!(apps.len(), 2);
    assert_eq!(apps[0]["path"], "C:\\Start Menu\\Classic.lnk");
    assert_eq!(
        apps[1]["path"],
        "shell:AppsFolder\\Microsoft.Calculator!App"
    );
    assert_eq!(apps[1]["windowsAppId"], "Microsoft.Calculator!App");
    let docs = providers::app_documents(&apps);
    assert_eq!(
        docs[1]["id"],
        "app:shell:AppsFolder\\Microsoft.Calculator!App"
    );
}
#[cfg(target_os = "macos")]
#[tokio::test]
async fn bundle_id_reads_temporary_plist_without_enumerating_installed_apps() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("Sample.app");
    std::fs::create_dir_all(app.join("Contents")).unwrap();
    let plist = app.join("Contents/Info.plist");
    std::fs::write(&plist,r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.example.Sample</string></dict></plist>"#).unwrap();
    assert_eq!(
        providers::bundle_id(app.to_str().unwrap()).await,
        "com.example.Sample"
    );
    providers::output(
        "/usr/bin/plutil",
        &["-convert", "binary1", plist.to_str().unwrap()],
        2,
    )
    .await
    .unwrap();
    assert_eq!(
        providers::bundle_id(app.to_str().unwrap()).await,
        "com.example.Sample"
    );
    assert!(
        providers::bundle_id(dir.path().join("Missing.app").to_str().unwrap())
            .await
            .is_null()
    );
}

#[tokio::test]
async fn hotkeys_persist_registered_accelerator_and_rollback_on_write_failure() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    ctx.config
        .patch(&json!({"commandHotkeys":{"cmd":"Control+O"},"future":42}))
        .unwrap();
    let calls = Arc::new(Mutex::new(vec![]));
    let sink = calls.clone();
    let path = ctx.config.path().to_owned();
    ctx.set_router(Arc::new(move |channel, args| {
        assert_eq!(channel, "core:shortcut:update-command");
        let sink = sink.clone();
        let path = path.clone();
        Box::pin(async move {
            sink.lock().unwrap().push(args.clone());
            if args[1] == "break-save" {
                // Simulate config becoming unwritable between registration and commit.
                std::fs::remove_file(&path).unwrap();
                std::fs::create_dir(&path).unwrap();
            }
            Ok(json!({"ok":true,"accelerator":if args[1]=="CmdOrCtrl+K" {json!("Control+K")}else{args[1].clone()}}))
        })
    })).unwrap();
    let core = LauncherCore::new(ctx.clone());
    let r = core
        .dispatch(
            "update-command-hotkey",
            &[json!("cmd"), json!("CmdOrCtrl+K")],
        )
        .await
        .unwrap();
    assert_eq!(r["accelerator"], "Control+K");
    assert_eq!(
        ctx.config.read().unwrap()["commandHotkeys"]["cmd"],
        "Control+K"
    );
    assert_eq!(ctx.config.read().unwrap()["future"], 42);
    assert!(core
        .dispatch(
            "update-command-hotkey",
            &[json!("cmd"), json!("break-save")]
        )
        .await
        .is_err());
    assert_eq!(
        calls.lock().unwrap().last().unwrap(),
        &vec![json!("cmd"), json!("Control+K")]
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelling_shortcut_ipc_still_persists_the_registered_binding() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let handoff = Arc::new(Mutex::new(Some((started, released))));
    ctx.set_router(Arc::new(move |channel, _| {
        assert_eq!(channel, "core:shortcut:update-command");
        let (started, released) = handoff.lock().unwrap().take().unwrap();
        Box::pin(async move {
            started.send(()).unwrap();
            released.await.unwrap();
            Ok(json!({"ok":true,"accelerator":"Control+K","previousAccelerator":""}))
        })
    }))
    .unwrap();
    let core = LauncherCore::new(ctx.clone());
    let request_core = core.clone();
    let request = tokio::spawn(async move {
        request_core
            .dispatch(
                "update-command-hotkey",
                &[json!("cmd"), json!("CmdOrCtrl+K")],
            )
            .await
    });
    ready.await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    // The transaction already holds this lock while the OS registration waits.
    // Its release establishes completion without relying on a sleep or polling.
    let _committed = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        core.inner.shortcuts.lock(),
    )
    .await
    .unwrap();
    assert_eq!(
        ctx.config.read().unwrap()["commandHotkeys"]["cmd"],
        "Control+K"
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn port_search_returns_cached_rows_during_slow_or_failed_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let core = LauncherCore::new(profile(dir.path()));
    let fixture = json!({"pid":"123","process":"fixture","ports":[3000]});
    *core.inner.ports.lock().await = (1, vec![fixture.clone()]);
    let (started, ready) = tokio::sync::oneshot::channel();
    let (finish, done) = tokio::sync::oneshot::channel::<()>();
    let rows = core.search_ports(async move {
        let _ = started.send(());
        let _ = done.await;
        Err("fixture process enumeration failure".into())
    });
    assert_eq!(rows, json!([fixture.clone()]));
    ready.await.unwrap();
    assert_eq!(
        core.search_ports(async { panic!("duplicate refresh must not run") }),
        json!([fixture])
    );
    finish.send(()).unwrap();
    core.shutdown().await.unwrap();
}
