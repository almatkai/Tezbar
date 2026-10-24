use super::*;
use crate::native_core::common::CorePaths;
fn source_root() -> PathBuf {
    let file = PathBuf::from(file!());
    let file = if file.is_absolute() {
        file
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file)
    };
    file.ancestors().nth(5).unwrap().to_owned()
}
fn profile(dir: &Path) -> CoreContext {
    CoreContext::headless(CorePaths {
        data_dir: dir.join("data"),
        config_dir: dir.join("config"),
        resource_dir: dir.join("resources"),
        temp_dir: dir.join("tmp"),
        home_dir: dir.join("home"),
    })
}

#[tokio::test]
async fn source_search_route_keeps_files_when_gemma_is_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    let docs = dir.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    core.database().await.unwrap().call(move |s| {
        s.root_add(&docs)?;
        let root = s.roots()?[0]["id"].as_str().unwrap().to_owned();
        let mut settings = s.settings()?;
        settings["embeddingModel"] = json!("embeddinggemma-2");
        s.set_meta("settings", &settings)?;
        for i in 0..4 {
            let id = format!("source-{i}");
            let pages = if i == 0 {
                (1..=100).map(|n| json!({"pageNumber":n,"ocr":{"text":"Материалы учителя"}})).collect::<Vec<_>>()
            } else {
                vec![json!({"pageNumber":1,"ocr":{"text":format!("Материалы учителя. {}", "Описание учебного занятия. ".repeat(20))}})]
            };
            s.pending(&id, &root, &docs.join(format!("{i}.png")).to_string_lossy(), &json!({"contentHash":id,"byteSize":100,"modifiedAt":0}), "smart")?;
            s.save_result(&id, &json!({"chunks":text::chunks(&id,&pages,true),"pages":pages,"images":[]}))?;
        }
        Ok(())
    }).await.unwrap();
    let args = [json!("Материалы учителя"), json!(3)];
    let chunks = core
        .handle("core:knowledge:search", &args)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(chunks.as_array().unwrap().len(), 3);
    assert!(chunks
        .as_array()
        .unwrap()
        .iter()
        .all(|h| h["sourceId"] == "source-0"));
    let files = core
        .handle("core:knowledge:search-sources", &args)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(files.as_array().unwrap().len(), 3);
    assert_eq!(
        files
            .as_array()
            .unwrap()
            .iter()
            .map(|h| string(h, "sourceId"))
            .collect::<HashSet<_>>()
            .len(),
        3
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn native_dispatch_uses_builtin_hybrid_without_learned_runtime() {
    for have_learned in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let core = KnowledgeCore::new(profile(dir.path()));
        let docs = dir.path().join("docs");
        std::fs::create_dir(&docs).unwrap();
        core.database().await.unwrap().call(move |s| {
            s.root_add(&docs)?;
            let root = s.roots()?[0]["id"].as_str().unwrap().to_owned();
            let mut settings = s.settings()?;
            settings["embeddingModel"] = json!("embeddinggemma-2");
            s.set_meta("settings", &settings)?;
            let pages = vec![json!({"pageNumber":1,"extractedText":"archived unrelated record"})];
            s.pending("semantic", &root, &docs.join("semantic.txt").to_string_lossy(), &json!({"contentHash":"x","byteSize":1,"modifiedAt":0}), "smart")?;
            s.save_result("semantic", &json!({"chunks":text::chunks("semantic", &pages, true),"pages":pages,"images":[]}))?;
            s.db.execute("UPDATE knowledge_chunks SET embedding_json=?", [text::encode(&text::embed("espresso calibration"))]).map_err(|e|e.to_string())?;
            if have_learned {
                let chunks = s.gemma_pending(&[root], 10)?;
                s.gemma_save(&chunks, &vec![vec![1.;256]; chunks.len()])?;
            }
            Ok(())
        }).await.unwrap();
        for channel in ["core:knowledge:search-sources", "core:knowledge:search"] {
            let hits = core
                .handle(channel, &[json!("espresso calibration"), json!(16)])
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                hits[0]["sourceId"], "semantic",
                "learned={have_learned}, {channel}"
            );
            assert!(hits[0]["semanticScore"].as_f64().unwrap() > 0.99);
        }
        core.shutdown().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn database_queue_supersession_and_chunk_isolation() {
    use std::future::Future;
    use std::task::Poll;
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(profile(dir.path())).await.unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let worker = db.clone();
    let blocker = tokio::spawn(async move {
        worker
            .call(move |_| {
                let _ = entered.send(());
                blocked.recv().unwrap();
                Ok(())
            })
            .await
    });
    started.await.unwrap();
    let source_epoch = Arc::new(AtomicU64::new(0));
    let chunk_epoch = Arc::new(AtomicU64::new(0));
    let old = db::SearchGuard::new(source_epoch.clone());
    let runs = Arc::new(AtomicU64::new(0));
    let count = runs.clone();
    let mut stale = Box::pin(db.call_search(old.0.clone(), move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    std::future::poll_fn(|cx| {
        assert!(stale.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let dropped = db::SearchGuard::new(chunk_epoch.clone());
    let count = runs.clone();
    let mut no_receiver = Box::pin(db.call_search(dropped.0.clone(), move |_| {
        count.fetch_add(100, Ordering::SeqCst);
        Ok(())
    }));
    std::future::poll_fn(|cx| {
        assert!(no_receiver.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(no_receiver);
    let chunk = db::SearchGuard::new(chunk_epoch);
    let mut independent = Box::pin(db.call_search(chunk.0.clone(), |_| Ok("chunk")));
    std::future::poll_fn(|cx| {
        assert!(independent.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let latest = db::SearchGuard::new(source_epoch);
    let mut newest = Box::pin(db.call_search(latest.0.clone(), |_| Ok("latest")));
    std::future::poll_fn(|cx| {
        assert!(newest.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    release.send(()).unwrap();
    blocker.await.unwrap().unwrap();
    assert!(stale.await.is_err());
    assert_eq!(independent.await.unwrap(), "chunk");
    assert_eq!(newest.await.unwrap(), "latest");
    assert_eq!(
        runs.load(Ordering::SeqCst),
        0,
        "superseded and dropped jobs must never run"
    );
    db.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_search_domains_supersede_only_their_own_callers() {
    use std::future::Future;
    use std::task::Poll;
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    core.start().await.unwrap();
    let db = core.database().await.unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let blocker = tokio::spawn(async move {
        db.call(move |_| {
            let _ = entered.send(());
            blocked.recv().unwrap();
            Ok(())
        })
        .await
    });
    started.await.unwrap();
    let mut old = Box::pin(core.search("old".into(), 16, true));
    std::future::poll_fn(|cx| {
        assert!(old.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let mut chunks = Box::pin(core.search("chunks".into(), 16, false));
    std::future::poll_fn(|cx| {
        assert!(chunks.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let mut latest = Box::pin(core.search("latest".into(), 16, true));
    std::future::poll_fn(|cx| {
        assert!(latest.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    release.send(()).unwrap();
    blocker.await.unwrap().unwrap();
    assert!(old.await.is_err());
    assert!(chunks.await.is_ok());
    assert!(latest.await.is_ok());
    core.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_database_reply_cancels_running_search_without_mutation_cancel() {
    use std::future::Future;
    use std::task::Poll;
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(profile(dir.path())).await.unwrap();
    let request = db::SearchGuard::new(Arc::default());
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let mut query = Box::pin(db.call_search(request.0.clone(), move |s| {
        let _ = entered.send(());
        blocked.recv().unwrap();
        s.search_sources("espresso", 16, &["r".into()], false, None)
    }));
    std::future::poll_fn(|cx| {
        assert!(query.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    started.await.unwrap();
    drop(query);
    release.send(()).unwrap();
    db.call(|s| {
        s.search_checkpoint()?;
        s.set_meta("mutation-survived", &json!(true))
    })
    .await
    .unwrap();
    assert!(db
        .call(|s| s.meta("mutation-survived", json!(false)))
        .await
        .unwrap()
        .as_bool()
        .unwrap());
    db.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sqlite_search_cancellation_is_scoped_and_does_not_interrupt_next_commit() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(profile(dir.path())).await.unwrap();
    db.call(|s| {
        s.db.execute_batch("CREATE TABLE cancellation_commit_test(id INTEGER PRIMARY KEY)")
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap();
    let epoch = Arc::new(AtomicU64::new(0));
    let old = db::SearchGuard::new(epoch.clone());
    let token = old.0.clone();
    let (entered, started) = tokio::sync::oneshot::channel();
    let worker = db.clone();
    let query = tokio::spawn(async move {
        worker.call_search(token, move |s| {
        let _ = entered.send(());
        s.db.query_row("WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT SUM(x) FROM n", [], |r|r.get::<_,i64>(0)).map_err(|e|e.to_string())
    }).await
    });
    started.await.unwrap();
    let _latest = db::SearchGuard::new(epoch);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), query)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    // This commit executes many VM steps with the old token still superseded.
    // A leaked callback or connection-wide interrupt would abort the mutation.
    db.call(|s| {
        let tx=s.db.transaction().map_err(|e|e.to_string())?;
        tx.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000) INSERT INTO cancellation_commit_test SELECT x FROM n").map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())
    }).await.unwrap();
    assert_eq!(
        db.call(|s| s
            .db
            .query_row("SELECT COUNT(*) FROM cancellation_commit_test", [], |r| r
                .get::<_, i64>(
                0
            ))
            .map_err(|e| e.to_string()))
            .await
            .unwrap(),
        10000
    );
    db.close().await.unwrap();
}

#[test]
fn utf16_feature_and_chunk_parity() {
    assert_eq!(text::embed(""), vec![0.; 384]);
    assert_eq!(text::embed("Hello WORLD"), text::embed("hello world"));
    assert!(
        (text::cosine(&text::embed("Kazakh қазақ"), &text::embed("Kazakh қазақ")) - 1.).abs()
            < 1e-10
    );
    let pages = vec![json!({"pageNumber":1,"extractedText":"A😀 мәтін. English text"})];
    let chunks = text::chunks("source", &pages, true);
    assert_eq!(chunks[0]["endOffset"], 23);
    assert_eq!(
        chunks[0]["id"],
        text::hash("source:1:0:A😀 мәтін. English text")
    );
    assert_eq!(
        text::profile("basic"),
        r#"{"depth":"basic","capabilities":["extracted-text","chunks"],"maxPagesPerDocument":20,"maxOcrPagesPerDocument":0,"ocrEveryPage":false}"#
    );
}
#[tokio::test]
async fn indexes_searches_reads_and_reopens_legacy_database() {
    let dir = tempfile::tempdir().unwrap();
    let docs = dir.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    std::fs::write(
        docs.join("manual.md"),
        "Quantum zebras explain the espresso calibration procedure. token=private123",
    )
    .unwrap();
    std::fs::write(docs.join("credentials.txt"), "Never index secrets").unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    assert!(core.handle("unowned", &[]).await.is_none());
    core.handle("knowledge:root:add", &[json!(docs)])
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if core.background()["status"]["state"] == "completed" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let hits = core
        .handle("core:knowledge:search", &[json!("espresso"), json!(10)])
        .await
        .unwrap()
        .unwrap();
    let hit = &hits[0];
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert!(string(hit, "text").contains("token=[REDACTED]"));
    assert!(!string(hit, "text").contains("private123"));
    let read = core
        .handle(
            "core:knowledge:read",
            &[hit["chunkId"].clone(), json!(12000)],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read["sourceId"], hit["sourceId"]);
    let sources = core
        .handle("knowledge:sources:list", &[json!({})])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sources["total"], 1);
    let metadata = core
        .handle(
            "core:knowledge:search-metadata",
            &[json!("manual"), json!(20)],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata[0]["sourceId"], hit["sourceId"]);
    assert_eq!(metadata[0]["title"], "manual.md");
    assert_eq!(metadata[0]["score"], 1.0);
    let root = core.snapshot()["roots"][0]["id"].clone();
    core.handle(
        "knowledge:root:set-enabled",
        &[json!({"rootId":root,"enabled":false})],
    )
    .await
    .unwrap()
    .unwrap();
    assert!(core
        .handle(
            "core:knowledge:search-metadata",
            &[json!("manual"), json!(20)]
        )
        .await
        .unwrap()
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert!(core
        .handle("core:knowledge:read", &[hit["chunkId"].clone()])
        .await
        .unwrap()
        .unwrap()
        .is_null());
    core.shutdown().await.unwrap();
    let reopened = KnowledgeCore::new(profile(dir.path()));
    reopened.start().await.unwrap();
    assert_eq!(reopened.snapshot()["status"]["sourceCount"], 1);
    reopened
        .handle("knowledge:root:remove", &[root])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reopened.snapshot()["status"]["chunkCount"], 0);
    reopened.shutdown().await.unwrap();
}
#[tokio::test]
async fn rejects_payloads_and_background_remains_cached() {
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    let b = core
        .handle("core:knowledge:background", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b["settings"]["embeddingModel"], "built-in");
    assert!(!dir.path().join("data").exists());
    for (channel, payload) in [
        ("knowledge:depth:set", json!("bogus")),
        (
            "knowledge:root:set-enabled",
            json!({"rootId":"x","enabled":"yes"}),
        ),
        ("knowledge:embedding:set", json!("embeddinggemma-2")),
    ] {
        assert!(core.handle(channel, &[payload]).await.unwrap().is_err());
    }
    core.shutdown().await.unwrap();
}
#[test]
fn learned_normalization_and_sensitive_path_rules() {
    let v = runtime::normalize(&json!(vec![1.; 768])).unwrap();
    assert_eq!(v.len(), 256);
    assert!(v.iter().all(|x| (*x - 0.0625).abs() < 1e-12));
    assert!(runtime::normalize(&json!(vec![0.; 768])).is_err());
    assert!(extract::skip(".env", false));
    assert!(extract::skip("access_token.txt", false));
    assert!(extract::skip("target", true));
    assert!(!extract::skip("notes.md", false));
}

#[tokio::test]
async fn durable_queue_recovers_paused_and_resumes_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let docs = dir.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    let file = docs.join("saved.txt");
    std::fs::write(&file, "Checkpoint recovery persists searchable documents").unwrap();
    let database = Database::open(profile(dir.path())).await.unwrap();
    let root_path = docs.clone();
    let path = file.to_string_lossy().to_string();
    database
        .call(move |s| {
            s.root_add(&root_path)?;
            let root = s.roots()?[0]["id"].as_str().unwrap().to_string();
            s.save_queue(&[(root, path)])?;
            let mut status = db::default_status();
            status["state"] = json!("paused");
            s.set_meta("status", &status)
        })
        .await
        .unwrap();
    database.close().await.unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    core.start().await.unwrap();
    assert_eq!(core.background()["status"]["state"], "paused");
    assert_eq!(core.background()["status"]["queuedSources"], 1);
    core.handle("knowledge:index:resume", &[])
        .await
        .unwrap()
        .unwrap();
    wait_completed(&core).await;
    let database = core.database().await.unwrap();
    database
        .call(|s| {
            assert!(s.checkpoint()?.is_none());
            let count: i64 =
                s.db.query_row("SELECT COUNT(*) FROM knowledge_chunks", [], |r| r.get(0))
                    .unwrap();
            assert_eq!(count, 1);
            Ok(())
        })
        .await
        .unwrap();
    core.shutdown().await.unwrap();
}
async fn wait_completed(core: &KnowledgeCore) {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let state = core.background()["status"]["state"].clone();
            assert_ne!(state, "failed", "{}", core.background());
            if state == "completed" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn vec0_and_missing_extension_round_trip_preserve_float_embeddings() {
    let root = source_root();
    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(windows) {
        "dll"
    } else {
        "so"
    };
    if !root.join(format!("dist-backend/vec0.{ext}")).is_file() {
        eprintln!("No bundled vec0 for this test platform");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = profile(dir.path());
    ctx.paths.resource_dir = root.clone();
    let database = Database::open(ctx.clone()).await.unwrap();
    database.call(|s|{assert!(s.vector,"Bundled vec0 must load with native SQLite");let path=s.path.parent().unwrap().join("root");std::fs::create_dir_all(&path).unwrap();s.root_add(&path)?;let root=s.roots()?[0]["id"].as_str().unwrap().to_owned();let source="fixture-source";s.pending(source,&root,"/fixture/document.txt",&json!({"contentHash":"fingerprint","byteSize":99,"modifiedAt":1}),&text::profile("smart"))?;let chunks=text::chunks(source,&[json!({"pageNumber":1,"extractedText":"Vec extension compatibility and native search"})],true);s.save_result(source,&json!({"pages":[{"pageNumber":1,"extractedText":"Vec extension compatibility and native search"}],"chunks":chunks,"images":[],"completedCapabilities":["extracted-text","chunks","text-embeddings"]}))?;
        let chunk=chunks[0].clone();let mut v=vec![0.;256];v[0]=1.;s.gemma_save(&[json!({"id":chunk["id"],"text":chunk["text"]})],&[v])?;s.vector_backfill()?;
        let mut query_vector=vec![0.;256];query_vector[0]=1.;
        let learned=s.search("compatibility",4,&[root.clone()],false,Some((&query_vector,&[root.clone()])))?;
        assert_eq!(learned.len(),1);assert!(learned[0]["semanticScore"].as_f64().unwrap()>0.99);
        assert_eq!(s.search("compatibility",4,&[root],false,None)?.len(),1);Ok(())}).await.unwrap();
    database.close().await.unwrap();
    let fallback = Database::open(profile(dir.path())).await.unwrap();
    fallback
        .call(|s| {
            assert!(!s.vector);
            let roots = s
                .roots()?
                .iter()
                .map(|r| string(r, "id").to_string())
                .collect::<Vec<_>>();
            assert_eq!(s.search("compatibility", 4, &roots, false, None)?.len(), 1);
            s.db.execute("DELETE FROM knowledge_chunks", [])
                .map_err(|e| e.to_string())?;
            let count: i64 =
                s.db.query_row("SELECT COUNT(*) FROM knowledge_gemma_embeddings", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(count, 0);
            Ok(())
        })
        .await
        .unwrap();
    fallback.close().await.unwrap();
    let restored = Database::open(ctx).await.unwrap();
    restored
        .call(|s| {
            assert!(s.vector);
            s.vector_backfill()?;
            let count: i64 =
                s.db.query_row(
                    "SELECT COUNT(*) FROM knowledge_chunk_vectors_binary",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
            Ok(())
        })
        .await
        .unwrap();
    restored.close().await.unwrap();
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn native_pdf_helper_protocol_retains_ocr_and_partial_page_metadata() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let helper = ctx
        .paths
        .resource_dir
        .join("native/screenocr/screenocr-helper");
    std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
    std::fs::write(&helper,r#"#!/usr/bin/env python3
import sys,json
assert sys.argv[1]=='extract-pdf'
args=json.loads(sys.argv[2]);assert args['maxPages']==20 and args['maxOcrPages']==0
print(json.dumps({'ok':True,'value':{'totalPages':42,'pages':[{'pageNumber':1,'extractedText':'PDF title','ocr':{'text':'Scanned text','blocks':[{'text':'OCR block','confidence':0.9}],'averageConfidence':0.9}}]}}))
"#).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cancel = Cancellation {
        generation: Arc::new(AtomicU64::new(0)),
        expected: 0,
    };
    let result = extract::extract(&ctx, &dir.path().join("file.pdf"), "basic", &cancel)
        .await
        .unwrap();
    assert_eq!(result["totalPageCount"], 42);
    assert_eq!(result["pages"][0]["ocr"]["blocks"][0]["confidence"], 0.9);
}

#[test]
fn generated_typescript_vectors_and_chunk_ids_match_rust() {
    // Captured from current featureEmbedding.ts and chunker.ts using Bun solely as a test oracle.
    let fixture: Value = serde_json::from_str(include_str!("fixtures/parity.json")).unwrap();
    for item in fixture["vectors"].as_array().unwrap() {
        let actual = text::embed(item["text"].as_str().unwrap());
        let expected: Vec<f64> = serde_json::from_value(item["vector"].clone()).unwrap();
        assert!(
            actual
                .iter()
                .zip(&expected)
                .all(|(a, b)| (a - b).abs() < 1e-14),
            "UTF-16/NFKC feature parity: {}",
            item["text"]
        );
    }
    let actual = text::chunks(
        "parity",
        &[json!({"pageNumber":3,"extractedText":fixture["text"]})],
        false,
    );
    assert_eq!(json!(actual), fixture["chunks"]);
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn packaged_pdfkit_helper_extracts_real_temporary_pdf() {
    let root = source_root();
    if !root.join("native/screenocr/screenocr-helper").is_file() {
        eprintln!("Packaged helper not built for this platform");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = profile(dir.path());
    ctx.paths.resource_dir = root.clone();
    let stream = "BT /F1 12 Tf 72 720 Td (Rust native PDF parity) Tj ET";
    let objects=vec!["<< /Type /Catalog /Pages 2 0 R >>".to_owned(),"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_owned(),format!("<< /Length {} >>\nstream\n{stream}\nendstream",stream.len()),"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned()];
    let mut pdf = "%PDF-1.4\n".to_owned();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let start = pdf.len();
    pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n"
    ));
    let path = dir.path().join("fixture.pdf");
    std::fs::write(&path, pdf).unwrap();
    let cancel = Cancellation {
        generation: Arc::new(AtomicU64::new(0)),
        expected: 0,
    };
    let result = extract::extract(&ctx, &path, "basic", &cancel)
        .await
        .unwrap();
    assert_eq!(result["totalPageCount"], 1);
    assert!(result["pages"][0]["extractedText"]
        .as_str()
        .unwrap()
        .contains("Rust native PDF parity"));
}

#[tokio::test]
async fn native_watcher_reindexes_changed_files_and_removes_deleted_sources() {
    let dir = tempfile::tempdir().unwrap();
    let docs = dir.path().join("watched");
    std::fs::create_dir(&docs).unwrap();
    let path = docs.join("watch.txt");
    std::fs::write(&path, "Original alpha document").unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    core.handle("knowledge:root:add", &[json!(docs)])
        .await
        .unwrap()
        .unwrap();
    wait_completed(&core).await;
    std::fs::write(
        &path,
        "Replacement document with unique watcherupdate content",
    )
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(18), async {
        loop {
            let db = core.database().await.unwrap();
            let rows = db
                .call(|s| {
                    s.search(
                        "watcherupdate",
                        10,
                        &s.roots()?
                            .iter()
                            .map(|r| string(r, "id").to_owned())
                            .collect::<Vec<_>>(),
                        true,
                        None,
                    )
                })
                .await
                .unwrap();
            if !rows.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|error| {
        panic!(
            "{error}: snapshot={} dirty={} now={} requested={} run_free={} watcher={}",
            core.snapshot(),
            core.inner.dirty.load(Ordering::SeqCst),
            now_ms(),
            core.inner.requested.load(Ordering::SeqCst),
            core.inner.run.try_lock().is_ok(),
            core.inner.watcher.lock().unwrap().is_some()
        )
    });
    std::fs::remove_file(&path).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(18), async {
        loop {
            if core.background()["status"]["sourceCount"] == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(core.snapshot()["status"]["chunkCount"], 0);
    core.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn extractor_drains_large_stderr_without_breaking_helper() {
    let cancel = Cancellation {
        generation: Arc::new(AtomicU64::new(0)),
        expected: 0,
    };
    let mut cmd = tokio::process::Command::new("python3");
    cmd.args([
        "-c",
        "import os; [os.write(2,b'x'*8192) for _ in range(128)]; print('completed')",
    ]);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        extract::command(cmd, &cancel),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, b"completed\n");
}

#[cfg(unix)]
#[tokio::test]
async fn extractor_cancellation_terminates_descendant_converter() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let generation = Arc::new(AtomicU64::new(0));
    let cancel = Cancellation {
        generation: generation.clone(),
        expected: 0,
    };
    let mut cmd = tokio::process::Command::new("python3");
    cmd.args(["-c", "import subprocess,sys,time,pathlib; p=subprocess.Popen(['sleep','30']); pathlib.Path(sys.argv[1]).write_text(str(p.pid)); time.sleep(30)"]).arg(&pidfile);
    let task = tokio::spawn(async move { extract::command(cmd, &cancel).await });
    let pid: i32 = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(raw) = std::fs::read_to_string(&pidfile) {
                if let Ok(pid) = raw.parse::<i32>() {
                    break pid;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    generation.fetch_add(1, Ordering::SeqCst);
    assert!(task.await.unwrap().unwrap_err().contains("cancelled"));
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while unsafe { libc::kill(pid, 0) } == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    // Do not leak the test fixture even if the assertion fails.
    if stopped.is_err() {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
    assert!(stopped.is_ok(), "Converter survived cancellation");
}

#[tokio::test]
async fn additive_legacy_migration_and_failed_chunk_replacement_preserve_content() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let directory = ctx.paths.data_dir.join("knowledge");
    std::fs::create_dir_all(&directory).unwrap();
    let legacy = rusqlite::Connection::open(directory.join("knowledge.sqlite3")).unwrap();
    legacy.execute_batch(r#"CREATE TABLE knowledge_roots(id TEXT PRIMARY KEY,path TEXT NOT NULL UNIQUE,depth TEXT NOT NULL,processing_backend TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
      CREATE TABLE knowledge_sources(id TEXT PRIMARY KEY,root_id TEXT NOT NULL,path TEXT NOT NULL UNIQUE,content_hash TEXT NOT NULL,byte_size INTEGER NOT NULL,modified_at INTEGER NOT NULL,media_type TEXT,status TEXT NOT NULL,error TEXT,indexed_at INTEGER);
      CREATE TABLE knowledge_pages(source_id TEXT NOT NULL,page_number INTEGER NOT NULL,extracted_text TEXT,ocr_text TEXT,ocr_blocks_json TEXT,ocr_confidence REAL,PRIMARY KEY(source_id,page_number));
      CREATE TABLE knowledge_chunks(id TEXT PRIMARY KEY,source_id TEXT NOT NULL,page_number INTEGER,text TEXT NOT NULL,embedding_json TEXT,start_offset INTEGER,end_offset INTEGER);
      CREATE TABLE knowledge_metadata(key TEXT PRIMARY KEY,value_json TEXT NOT NULL);
      INSERT INTO knowledge_roots VALUES('root','/fixture','text','local',1,1,1);
      INSERT INTO knowledge_sources VALUES('source','root','/fixture/legacy.txt','hash',20,1,'txt','indexed',NULL,1);
      INSERT INTO knowledge_pages VALUES('source',1,'Legacy durable content',NULL,NULL,NULL);
      INSERT INTO knowledge_chunks VALUES('chunk','source',1,'Legacy durable content','[1,0,0]',0,22);
      INSERT INTO knowledge_metadata VALUES('status','{"sourceCount":1,"chunkCount":1,"sourceBytes":20}');"#).unwrap();
    drop(legacy);
    let database = Database::open(ctx).await.unwrap();
    database.call(|s| {
        assert_eq!(s.roots()?[0]["depth"], "inherit");
        assert_eq!(s.source("/fixture/legacy.txt")?.unwrap()["indexingProfile"], "");
        assert_eq!(s.search("durable", 10, &["root".into()], true, None)?.len(), 1);
        let before = s.counts()?;
        let duplicate = json!({"id":"duplicate","pageNumber":1,"text":"replacement"});
        // Failure occurs after deleting old content and inserting the first new chunk.
        let result = s.save_result("source", &json!({"pages":[{"pageNumber":1,"extractedText":"replacement"}],"chunks":[duplicate,duplicate]}));
        assert!(result.is_err());
        assert_eq!(s.counts()?, before);
        assert_eq!(s.search("durable", 10, &["root".into()], true, None)?[0]["chunkId"], "chunk");
        let page: String = s.db.query_row("SELECT extracted_text FROM knowledge_pages WHERE source_id='source'", [], |r| r.get(0)).unwrap();
        assert_eq!(page, "Legacy durable content");
        let embedding: String = s.db.query_row("SELECT embedding_json FROM knowledge_chunks WHERE id='chunk'", [], |r| r.get(0)).unwrap();
        assert_eq!(embedding, "[1,0,0]");
        Ok(())
    }).await.unwrap();
    database.close().await.unwrap();
}

#[tokio::test]
async fn q4_service_snapshot_defaults_and_unknown_ipc_are_strict() {
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    let snapshot = core
        .handle("knowledge:snapshot", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot["settings"]["embeddingVariant"], "q8_0");
    assert_eq!(snapshot["semanticSearch"]["variants"][0]["id"], "q8_0");
    assert_eq!(snapshot["semanticSearch"]["variants"][1]["id"], "q4_k_m");
    for channel in [
        "knowledge:embedding:set",
        "knowledge:embedding:download",
        "knowledge:embedding:cancel-download",
        "knowledge:embedding:remove",
    ] {
        let before = core.inner.generation.load(Ordering::SeqCst);
        let args = if channel.ends_with(":set") {
            vec![json!("built-in"), json!("unknown")]
        } else {
            vec![json!("unknown")]
        };
        assert!(
            core.handle(channel, &args).await.unwrap().is_err(),
            "{channel}"
        );
        assert_eq!(core.inner.generation.load(Ordering::SeqCst), before);
    }
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn q4_store_keeps_composite_rows_in_fresh_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(profile(dir.path())).await.unwrap();
    db.call(|s| {
        s.db.execute("INSERT INTO knowledge_roots VALUES('r','/fixture','smart','local',1,0,0)",[]).unwrap();
        s.db.execute("INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,status) VALUES('s','r','/fixture/a','hash',1,0,'indexed')",[]).unwrap();
        s.db.execute("INSERT INTO knowledge_chunks(id,source_id,text,embedding_json) VALUES('c','s','OCR сохранён','[1]')",[]).unwrap();
        for key in [runtime::MODEL_KEY, "unknown-retained-key"] {
            s.db.execute("INSERT OR REPLACE INTO knowledge_gemma_embeddings VALUES('c',?,X'00010203')",[key]).unwrap();
        }
        assert_eq!(s.db.query_row("SELECT COUNT(*) FROM knowledge_gemma_embeddings",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        Ok(())
    }).await.unwrap();
    db.close().await.unwrap();
}

async fn variant_fixture(core: &KnowledgeCore) -> Vec<String> {
    let docs = core.inner.ctx.paths.data_dir.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    core.database().await.unwrap().call(move |s| {
        s.root_add(&docs)?;
        let root = string(&s.roots()?[0],"id").to_owned();
        for i in 0..3 {
            let id = format!("variant-{i}");
            let pages = vec![json!({"pageNumber":1,"extractedText":format!("архив документа {i}"),"ocr":{"text":"Материалы учителя. План урока","blocks":[{"text":"план","confidence":0.9}],"averageConfidence":0.9}})];
            s.pending(&id,&root,&docs.join(format!("{i}.txt")).to_string_lossy(),&json!({"contentHash":id,"byteSize":10,"modifiedAt":1}),"smart")?;
            s.save_result(&id,&json!({"pages":pages,"chunks":text::chunks(&id,&pages,true),"images":[]}))?;
        }
        s.save_queue(&[(root.clone(),docs.join("next.txt").to_string_lossy().into_owned())])?;
        Ok(vec![root])
    }).await.unwrap()
}

#[tokio::test]
async fn q4_multimodel_pending_counts_remove_and_vec0_identity() {
    for accelerated in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = profile(dir.path());
        if accelerated {
            ctx.paths.resource_dir = source_root();
        }
        let core = KnowledgeCore::new(ctx);
        let roots = variant_fixture(&core).await;
        core.database().await.unwrap().call(move |s| {
            assert_eq!(s.vector,accelerated);
            let q8 = runtime::MODEL_KEY;
            let q4 = EmbeddingVariantId::Q4KM.descriptor().model_key;
            let chunks = s.gemma_pending_for_model(&roots,100,q8)?;
            assert_eq!(chunks.len(),3);
            let mut a=vec![0.;256]; a[0]=1.;
            let mut b=vec![0.;256]; b[1]=1.;
            let before:Vec<(String,Vec<u8>)> = s.db.prepare("SELECT id,CAST(text AS BLOB) FROM knowledge_chunks ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();
            s.gemma_save_for_model(&chunks,&[a.clone(),b.clone(),b.clone()],q8)?;
            assert!(s.gemma_pending_for_model(&roots,100,q8)?.is_empty());
            assert_eq!(s.gemma_pending_for_model(&roots,100,q4)?.len(),3);
            s.gemma_save_for_model(&chunks,&[b.clone(),a.clone(),b.clone()],q4)?;
            // Unknown keys survive recognized-model writes/removal too.
            s.gemma_save_for_model(&chunks[..1],&[a.clone()],"retained-unknown")?;
            assert_eq!(s.gemma_counts_for_model(&roots,q8)?,(3,3));
            assert_eq!(s.gemma_counts_for_model(&roots,q4)?,(3,3));
            assert!(s.gemma_has_for_model(&roots,q4)?);
            assert!(s.gemma_pending_for_model(&roots,100,q4)?.is_empty());
            for (key,winner) in [(q8,&chunks[0]),(q4,&chunks[1]),(q8,&chunks[0])] {
                let hits=s.search_with_model("no-lexical-match",3,&roots,false,Some((&a,&roots,key)))?;
                assert_eq!(hits[0]["chunkId"],winner["id"]);
                assert!(hits[0]["semanticScore"].as_f64().unwrap()>0.99);
                let cold=s.search_sources_with_model("no-lexical-match",3,&roots,false,Some((&a,&roots,key)))?;
                s.prewarm_source_cache(&roots,&roots,Some(key),256)?;
                while s.search_cache_maintenance()? {}
                let warm=s.search_sources_with_model("no-lexical-match",3,&roots,false,Some((&a,&roots,key)))?;
                assert_eq!(cold,warm);
            }
            if accelerated {
                assert_eq!(s.db.query_row("SELECT COUNT(*) FROM knowledge_gemma_vectors",[],|r|r.get::<_,i64>(0)).unwrap(),3);
                assert_eq!(s.db.query_row("SELECT COUNT(*) FROM knowledge_gemma_vectors_q4",[],|r|r.get::<_,i64>(0)).unwrap(),3);
                s.db.execute_batch("DELETE FROM knowledge_gemma_vectors_q4; DELETE FROM knowledge_metadata WHERE key LIKE 'rust-gemma-vector-cursor-v2:%'").unwrap();
                while s.vector_backfill_for_model(q4)? {}
                assert_eq!(s.search_with_model("no-lexical-match",3,&roots,false,Some((&a,&roots,q4)))?[0]["chunkId"],chunks[1]["id"]);
            }
            s.gemma_clear_for_model(q4)?;
            assert_eq!(s.gemma_counts_for_model(&roots,q4)?,(0,3));
            assert_eq!(s.gemma_counts_for_model(&roots,q8)?,(3,3));
            assert_eq!(s.gemma_counts_for_model(&roots,"retained-unknown")?,(1,3));
            let after:Vec<(String,Vec<u8>)> = s.db.prepare("SELECT id,CAST(text AS BLOB) FROM knowledge_chunks ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();
            assert_eq!(before,after);
            assert!(s.checkpoint()?.is_some());
            // Chunk deletion removes both model-specific derived rows.
            s.gemma_save_for_model(&chunks,&[b.clone(),a.clone(),b],q4)?;
            s.root_remove(&roots[0])?;
            assert_eq!(s.db.query_row("SELECT COUNT(*) FROM knowledge_gemma_embeddings",[],|r|r.get::<_,i64>(0)).unwrap(),0);
            if accelerated {
                for table in ["knowledge_gemma_vectors","knowledge_gemma_vectors_q4"] {
                    assert_eq!(s.db.query_row(&format!("SELECT COUNT(*) FROM {table}"),[],|r|r.get::<_,i64>(0)).unwrap(),0);
                }
            }
            Ok(())
        }).await.unwrap();
        core.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn q4_actual_store_open_migrates_legacy_q8_without_text_or_checkpoint_changes() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = profile(dir.path());
    let path = ctx.paths.data_dir.join("knowledge/knowledge.sqlite3");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let legacy=include_str!("schema.sql").replace("chunk_id TEXT NOT NULL REFERENCES knowledge_chunks(id) ON DELETE CASCADE,\n        model_key TEXT NOT NULL,\n        vector BLOB NOT NULL,\n        PRIMARY KEY(chunk_id, model_key)","chunk_id TEXT PRIMARY KEY,\n        model_key TEXT NOT NULL,\n        vector BLOB NOT NULL");
    db.execute_batch(&legacy).unwrap();
    db.execute_batch("INSERT INTO knowledge_roots VALUES('r','/legacy','smart','local',1,0,0); INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,status) VALUES('s','r','/legacy/a','h',1,0,'indexed'); INSERT INTO knowledge_chunks(id,source_id,text,embedding_json) VALUES('c','s','OCR legacy','[1,2]'); INSERT INTO knowledge_pages(source_id,page_number,extracted_text,ocr_text,ocr_blocks_json) VALUES('s',1,'text','OCR','[]'); INSERT INTO knowledge_indexing_jobs VALUES(1,'job',2,1,0,0); INSERT INTO knowledge_indexing_queue VALUES('job',1,'r','/legacy/next');").unwrap();
    db.execute(
        "INSERT INTO knowledge_gemma_embeddings VALUES('c',?,X'0001FF00')",
        [runtime::MODEL_KEY],
    )
    .unwrap();
    db.execute_batch("INSERT INTO knowledge_chunks(id,source_id,text) VALUES('u','s','unknown'); INSERT INTO knowledge_gemma_embeddings VALUES('u','unknown-key',X'040506');").unwrap();
    drop(db);
    let database = Database::open(ctx).await.unwrap();
    database
        .call(|s| {
            assert_eq!(
                s.db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                s.db.query_row(
                    "SELECT vector FROM knowledge_gemma_embeddings WHERE chunk_id='c'",
                    [],
                    |r| r.get::<_, Vec<u8>>(0)
                )
                .unwrap(),
                vec![0, 1, 255, 0]
            );
            s.db.execute(
                "INSERT INTO knowledge_gemma_embeddings VALUES('c',?,X'0809')",
                [EmbeddingVariantId::Q4KM.descriptor().model_key],
            )
            .unwrap();
            assert_eq!(
                s.db.query_row("SELECT COUNT(*) FROM knowledge_gemma_embeddings", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                3
            );
            assert_eq!(s.checkpoint()?.unwrap()["processedSources"], 1);
            assert_eq!(
                s.db.query_row(
                    "SELECT text,embedding_json FROM knowledge_chunks WHERE id='c'",
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                )
                .unwrap(),
                ("OCR legacy".into(), "[1,2]".into())
            );
            assert_eq!(
                s.db.query_row(
                    "SELECT extracted_text,ocr_text,ocr_blocks_json FROM knowledge_pages",
                    [],
                    |r| Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?
                    ))
                )
                .unwrap(),
                ("text".into(), "OCR".into(), "[]".into())
            );
            Ok(())
        })
        .await
        .unwrap();
    database.close().await.unwrap();
}

#[tokio::test]
async fn q4_ipc_omission_targets_q8_and_switch_keeps_extraction_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    let roots = variant_fixture(&core).await;
    core.start().await.unwrap();
    // Hold extraction ownership: a semantic choice must not await or cancel it.
    let _extraction = core.inner.run.lock().await;
    let extraction = core.cancel();
    let before = core
        .database()
        .await
        .unwrap()
        .call(|s| s.checkpoint())
        .await
        .unwrap();
    let snap = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        core.handle(
            "knowledge:embedding:set",
            &[json!("built-in"), json!("q4_k_m")],
        ),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(snap["settings"]["embeddingVariant"], "q4_k_m");
    extraction.check().unwrap();
    let epoch = core.inner.semantic_generation.load(Ordering::SeqCst);
    core.handle(
        "knowledge:embedding:set",
        &[json!("built-in"), json!("q4_k_m")],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(epoch, core.inner.semantic_generation.load(Ordering::SeqCst));
    // Distinct state sentinels prove omitted download/cancel do not route to Q4.
    core.runtime(EmbeddingVariantId::Q8_0)
        .patch(json!({"modelState":"ready","error":null}));
    core.runtime(EmbeddingVariantId::Q4KM)
        .patch(json!({"modelState":"error","errorSource":"runtime","error":"q4 sentinel"}));
    let snap = core
        .handle("knowledge:embedding:download", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        snap["semanticSearch"]["variants"][1]["failure"]["message"],
        "q4 sentinel"
    );
    let snap = core
        .handle("knowledge:embedding:cancel-download", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snap["settings"]["embeddingVariant"], "q4_k_m");
    let learned_roots = roots.clone();
    core.database()
        .await
        .unwrap()
        .call(move |s| {
            let chunks = s.gemma_pending(&learned_roots, 100)?;
            s.gemma_save(&chunks, &vec![vec![1.; 256]; chunks.len()])?;
            s.gemma_save_for_model(
                &chunks,
                &vec![vec![2.; 256]; chunks.len()],
                EmbeddingVariantId::Q4KM.descriptor().model_key,
            )?;
            Ok(())
        })
        .await
        .unwrap();
    // Removing omitted Q8 keeps the selected Q4 engine and vectors.
    core.inner.cache.lock().unwrap().settings["embeddingModel"] = json!("embeddinggemma-2");
    core.database()
        .await
        .unwrap()
        .call(|s| {
            let mut settings = s.settings()?;
            settings["embeddingModel"] = json!("embeddinggemma-2");
            s.set_meta("settings", &settings)
        })
        .await
        .unwrap();
    let snap = core
        .handle("knowledge:embedding:remove", &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snap["settings"]["embeddingModel"], "embeddinggemma-2");
    assert_eq!(snap["settings"]["embeddingVariant"], "q4_k_m");
    assert_eq!(snap["semanticSearch"]["indexedChunks"], 3);
    let snap = core
        .handle("knowledge:embedding:remove", &[json!("q4_k_m")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snap["settings"]["embeddingModel"], "built-in");
    let snap = core
        .handle("knowledge:embedding:set", &[json!("built-in")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snap["settings"]["embeddingVariant"], "q8_0");
    assert_eq!(
        core.database()
            .await
            .unwrap()
            .call(|s| s.checkpoint())
            .await
            .unwrap(),
        before
    );
    extraction.check().unwrap();
    drop(_extraction);
    core.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn q4_switch_fences_queued_search_and_learned_save_without_cancelling_extraction() {
    use std::{future::Future, task::Poll};
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    variant_fixture(&core).await;
    core.start().await.unwrap();
    let extraction = core.cancel();
    let semantic = core.semantic_cancel();
    let db = core.database().await.unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let worker = db.clone();
    let blocker = tokio::spawn(async move {
        worker
            .call(move |_| {
                let _ = entered.send(());
                blocked.recv().unwrap();
                Ok(())
            })
            .await
    });
    started.await.unwrap();
    let mut search = Box::pin(core.search("план урока".into(), 4, true));
    std::future::poll_fn(|cx| {
        assert!(search.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let condition = semantic.clone();
    let mut obsolete = Box::pin(db.call(move |s| {
        condition.check()?;
        s.set_meta("obsolete-save", &json!(true))
    }));
    std::future::poll_fn(|cx| {
        assert!(obsolete.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let switch_args = [json!("built-in"), json!("q4_k_m")];
    let mut switch = Box::pin(core.handle("knowledge:embedding:set", &switch_args));
    std::future::poll_fn(|cx| {
        assert!(switch.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    assert!(semantic.check().is_err());
    extraction.check().unwrap();
    release.send(()).unwrap();
    blocker.await.unwrap().unwrap();
    assert!(search.await.is_err());
    assert!(obsolete.await.is_err());
    switch.await.unwrap().unwrap();
    assert_eq!(
        db.call(|s| s.meta("obsolete-save", Value::Null))
            .await
            .unwrap(),
        Value::Null
    );
    core.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn q4_actual_learned_batch_is_cancelled_before_obsolete_model_publication() {
    use std::{future::Future, task::Poll};
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    variant_fixture(&core).await;
    core.start().await.unwrap();
    core.inner.cache.lock().unwrap().settings["embeddingModel"] = json!("embeddinggemma-2");
    let db = core.database().await.unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let worker = db.clone();
    let blocker = tokio::spawn(async move {
        worker
            .call(move |_| {
                let _ = entered.send(());
                blocked.recv().unwrap();
                Ok(())
            })
            .await
    });
    started.await.unwrap();
    let semantic = core.semantic_cancel();
    let mut batch = Box::pin(core.learned_batch(&semantic));
    std::future::poll_fn(|cx| {
        assert!(batch.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let extraction = core.cancel();
    let args = [json!("built-in"), json!("q4_k_m")];
    let mut switch = Box::pin(core.handle("knowledge:embedding:set", &args));
    std::future::poll_fn(|cx| {
        assert!(switch.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    release.send(()).unwrap();
    blocker.await.unwrap().unwrap();
    assert!(batch.await.is_err());
    switch.await.unwrap().unwrap();
    extraction.check().unwrap();
    assert_eq!(
        db.call(|s| Ok(s
            .db
            .query_row("SELECT COUNT(*) FROM knowledge_gemma_embeddings", [], |r| r
                .get::<_, i64>(0))
            .unwrap()))
            .await
            .unwrap(),
        0
    );
    assert!(core.runtime(EmbeddingVariantId::Q4KM).snapshot()["error"].is_null());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn q4_selected_runtime_failure_stays_visible_and_search_uses_builtin_hybrid() {
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    let roots = variant_fixture(&core).await;
    core.start().await.unwrap();
    let learned_roots = roots.clone();
    core.database()
        .await
        .unwrap()
        .call(move |s| {
            s.db.execute(
                "UPDATE knowledge_chunks SET embedding_json=?",
                [text::encode(&text::embed("espresso calibration"))],
            )
            .unwrap();
            let chunks = s.gemma_pending_for_model(
                &learned_roots,
                100,
                EmbeddingVariantId::Q4KM.descriptor().model_key,
            )?;
            s.gemma_save_for_model(
                &chunks,
                &vec![vec![1.; 256]; chunks.len()],
                EmbeddingVariantId::Q4KM.descriptor().model_key,
            )?;
            Ok(())
        })
        .await
        .unwrap();
    {
        let mut c = core.inner.cache.lock().unwrap();
        c.settings["embeddingModel"] = json!("embeddinggemma-2");
        c.settings["embeddingVariant"] = json!("q4_k_m");
    }
    core.runtime(EmbeddingVariantId::Q4KM).patch(json!({"modelState":"ready","integrityState":"verified","runtimeState":"error","errorSource":"runtime","error":"fixture startup failure"}));
    for channel in ["core:knowledge:search", "core:knowledge:search-sources"] {
        let began = std::time::Instant::now();
        let hits = core
            .handle(channel, &[json!("espresso calibration"), json!(6)])
            .await
            .unwrap()
            .unwrap();
        assert!(began.elapsed() < std::time::Duration::from_secs(2));
        assert!(hits[0]["semanticScore"].as_f64().unwrap() > 0.99);
    }
    let snap = core.snapshot();
    assert_eq!(snap["semanticSearch"]["errorSource"], "runtime");
    assert_eq!(snap["semanticSearch"]["runtimeState"], "error");
    assert_eq!(
        snap["semanticSearch"]["variants"][1]["failure"]["stage"],
        "startup"
    );
    assert!(snap["semanticSearch"]["variants"][0]
        .get("failure")
        .is_none());
    core.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn q4_old_snapshot_refresh_cannot_restore_previous_selection() {
    use std::{future::Future, task::Poll, time::Duration};
    let dir = tempfile::tempdir().unwrap();
    let core = KnowledgeCore::new(profile(dir.path()));
    core.start().await.unwrap();
    // Complete the old DB read but intentionally delay its service continuation.
    let mut old = Box::pin(core.refresh());
    std::future::poll_fn(|cx| {
        assert!(old.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    core.database()
        .await
        .unwrap()
        .call(|_| Ok(()))
        .await
        .unwrap();
    let switching = core.clone();
    let mut switch = tokio::spawn(async move {
        switching
            .handle(
                "knowledge:embedding:set",
                &[json!("built-in"), json!("q4_k_m")],
            )
            .await
            .unwrap()
    });
    // With serialized refresh the switch waits for the old continuation. Before
    // the fix it completes and then the delayed read overwrites its cache.
    let early = tokio::time::timeout(Duration::from_millis(100), &mut switch).await;
    old.await.unwrap();
    match early {
        Ok(result) => {
            result.unwrap().unwrap();
        }
        Err(_) => {
            switch.await.unwrap().unwrap();
        }
    }
    assert_eq!(core.settings()["embeddingVariant"], "q4_k_m");
    let persisted = core
        .database()
        .await
        .unwrap()
        .call(|s| s.settings())
        .await
        .unwrap();
    assert_eq!(core.settings(), persisted);
    core.shutdown().await.unwrap();
}
