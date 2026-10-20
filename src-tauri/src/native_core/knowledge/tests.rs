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
    .unwrap();
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
