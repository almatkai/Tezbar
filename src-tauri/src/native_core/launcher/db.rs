use super::text;
use crate::native_core::common::{now_ms, CoreResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
};
#[derive(Clone)]
pub struct Database {
    tx: mpsc::Sender<Message>,
    reader_tx: mpsc::Sender<Message>,
    threads: Arc<Mutex<Vec<std::thread::JoinHandle<()>>>>,
}
enum Message {
    Run(Job),
    Stop(tokio::sync::oneshot::Sender<()>),
}
type Job = Box<dyn FnOnce(Result<&mut Connection, String>) + Send>;
impl Database {
    pub fn new(path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();
        let (reader_tx, reader_rx) = mpsc::channel::<Message>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let writer_path = path.clone();
        let writer = std::thread::Builder::new()
            .name("launcher-sqlite-writer".into())
            .spawn(move || {
                let db = init(writer_path.clone());
                let _ = ready_tx.send(db.as_ref().map(|_| ()).map_err(Clone::clone));
                run(rx, db, || init(writer_path.clone()));
            })
            .expect("launcher database writer thread");
        let writer_tx = tx.clone();
        let reader = std::thread::Builder::new()
            .name("launcher-sqlite-reader".into())
            .spawn(move || {
                // Schema migration belongs exclusively to the writer.
                let db = ready_rx
                    .recv()
                    .map_err(err)
                    .and_then(|ready| ready)
                    .and_then(|_| open_reader(&path));
                run(reader_rx, db, || {
                    // A failed startup may need the writer to retry its migration.
                    let (ready_tx, ready_rx) = mpsc::channel();
                    writer_tx
                        .send(Message::Run(Box::new(move |db| {
                            let _ = ready_tx.send(db.map(|_| ()));
                        })))
                        .map_err(err)?;
                    ready_rx.recv().map_err(err)??;
                    open_reader(&path)
                });
            })
            .expect("launcher database reader thread");
        Self {
            tx,
            reader_tx,
            threads: Arc::new(Mutex::new(vec![writer, reader])),
        }
    }
    pub async fn call<F>(&self, f: F) -> CoreResult
    where
        F: FnOnce(&mut Connection) -> CoreResult + Send + 'static,
    {
        Self::dispatch(&self.tx, f).await
    }
    pub async fn read<F>(&self, f: F) -> CoreResult
    where
        F: FnOnce(&mut Connection) -> CoreResult + Send + 'static,
    {
        Self::dispatch(&self.reader_tx, f).await
    }
    async fn dispatch<F>(sender: &mpsc::Sender<Message>, f: F) -> CoreResult
    where
        F: FnOnce(&mut Connection) -> CoreResult + Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        sender
            .send(Message::Run(Box::new(move |db| {
                let _ = tx.send(db.and_then(f));
            })))
            .map_err(|_| "Launcher database stopped")?;
        rx.await.map_err(|e| e.to_string())?
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        let mut replies = Vec::new();
        for sender in [&self.reader_tx, &self.tx] {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if sender.send(Message::Stop(tx)).is_ok() {
                replies.push(rx);
            }
        }
        for reply in replies {
            let _ = reply.await;
        }
        let threads = std::mem::take(&mut *self.threads.lock().map_err(err)?);
        tokio::task::spawn_blocking(move || {
            let mut failed = false;
            for thread in threads {
                failed |= thread.join().is_err();
            }
            if failed {
                Err("Launcher database thread panicked".to_owned())
            } else {
                Ok(())
            }
        })
        .await
        .map_err(err)?
    }
    pub async fn upsert(&self, docs: Vec<Value>, replace: Vec<String>) -> CoreResult {
        self.call(move |db| {
            let tx = db.transaction().map_err(err)?;
            // Preserve unchanged rows (and their FTS entries) during periodic refresh.
            // The temporary ID set also avoids SQLite's parameter limit for large corpora.
            if !replace.is_empty() {
                tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS refresh_ids(id TEXT PRIMARY KEY); DELETE FROM refresh_ids;").map_err(err)?;
                for d in &docs {
                    tx.execute("INSERT OR IGNORE INTO refresh_ids(id) VALUES(?)", [d["id"].as_str().ok_or("Document id required")?]).map_err(err)?;
                }
                for category in replace {
                    tx.execute("DELETE FROM documents WHERE category=? AND id NOT IN (SELECT id FROM refresh_ids)", [category]).map_err(err)?;
                }
            }
            for d in docs {
                upsert(&tx, &d)?;
            }
            tx.commit().map_err(err)?;
            Ok(Value::Null)
        })
        .await
    }
    pub async fn search(&self, q: String, candidates: bool) -> CoreResult {
        self.read(move |db| {
            let snapshot = db.transaction().map_err(err)?;
            search(&snapshot, &q, candidates)
        })
        .await
    }
    pub async fn record(&self, id: String, context: Value, success: bool) -> CoreResult {
        self.call(move|db|{let tx=db.transaction().map_err(err)?;let now=now_ms();tx.execute("INSERT INTO action_stats(action_id,frequency,success_count,total_count,last_used_at) VALUES(?1,1,?2,1,?3) ON CONFLICT(action_id) DO UPDATE SET frequency=frequency+1, success_count=success_count+excluded.success_count,total_count=total_count+1,last_used_at=excluded.last_used_at",params![id,success,now]).map_err(err)?;if success{tx.execute("INSERT INTO action_usage_events(action_id,created_at) VALUES(?,?)",params![id,now]).map_err(err)?;}
 let q=text::normalize(context["query"].as_str().unwrap_or(""));if !q.is_empty()&&success{tx.execute("INSERT INTO query_action_stats(query,action_id,frequency,success_count,total_count,last_used_at) VALUES(?1,?2,1,?3,1,?4) ON CONFLICT(query,action_id) DO UPDATE SET frequency=frequency+1,success_count=success_count+excluded.success_count,total_count=total_count+1,last_used_at=excluded.last_used_at",params![q,id,success,now]).map_err(err)?;}
 if let Some(rank)=context["rank"].as_f64().filter(|n|n.is_finite()){tx.execute("INSERT INTO click_events(created_at,query,result_id,rank,success) VALUES(?,?,?,?,?)",params![now,q,id,rank.round() as i64,success]).map_err(err)?;}
 prune(&tx)?;tx.commit().map_err(err)?;Ok(Value::Null)}).await
    }
}
fn run(
    rx: mpsc::Receiver<Message>,
    mut db: Result<Connection, String>,
    reopen: impl Fn() -> Result<Connection, String>,
) {
    while let Ok(message) = rx.recv() {
        match message {
            Message::Run(job) => {
                if db.is_err() {
                    db = reopen();
                }
                job(db.as_mut().map_err(|e| e.clone()));
            }
            Message::Stop(reply) => {
                drop(db);
                let _ = reply.send(());
                break;
            }
        }
    }
}
fn open_reader(path: &std::path::Path) -> Result<Connection, String> {
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(err)?;
    db.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(err)?;
    Ok(db)
}
pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn init(path: PathBuf) -> Result<Connection, String> {
    std::fs::create_dir_all(path.parent().ok_or("Invalid DB path")?).map_err(err)?;
    let db = Connection::open(path).map_err(err)?;
    db.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(err)?;
    db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;
CREATE TABLE IF NOT EXISTS documents(id TEXT PRIMARY KEY,category TEXT NOT NULL,title TEXT NOT NULL,subtitle TEXT NOT NULL,tokens TEXT NOT NULL,action_json TEXT NOT NULL,updated_at INTEGER NOT NULL,source_path TEXT,source_mtime INTEGER,popularity REAL NOT NULL DEFAULT 0);
CREATE INDEX IF NOT EXISTS idx_documents_category ON documents(category);
CREATE INDEX IF NOT EXISTS idx_documents_nonfile_updated ON documents(updated_at DESC) WHERE category<>'files';
CREATE TABLE IF NOT EXISTS action_stats(action_id TEXT PRIMARY KEY,frequency INTEGER NOT NULL DEFAULT 0,success_count INTEGER NOT NULL DEFAULT 0,total_count INTEGER NOT NULL DEFAULT 0,last_used_at INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS query_action_stats(query TEXT NOT NULL,action_id TEXT NOT NULL,frequency INTEGER NOT NULL DEFAULT 0,success_count INTEGER NOT NULL DEFAULT 0,total_count INTEGER NOT NULL DEFAULT 0,last_used_at INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(query,action_id));
CREATE TABLE IF NOT EXISTS action_usage_events(id INTEGER PRIMARY KEY AUTOINCREMENT,action_id TEXT NOT NULL,created_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_action_usage_events_action_time ON action_usage_events(action_id,created_at DESC);
CREATE TABLE IF NOT EXISTS benchmark_snapshots(id INTEGER PRIMARY KEY AUTOINCREMENT,created_at INTEGER NOT NULL,precision_at_5 REAL NOT NULL,precision_at_10 REAL NOT NULL,avg_click_rank REAL NOT NULL,benchmark_size INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS click_events(id INTEGER PRIMARY KEY AUTOINCREMENT,created_at INTEGER NOT NULL,query TEXT NOT NULL,result_id TEXT NOT NULL,rank INTEGER NOT NULL,success INTEGER NOT NULL);").map_err(err)?;
    let columns: Vec<String> = db
        .prepare("PRAGMA table_info(documents)")
        .map_err(err)?
        .query_map([], |r| r.get(1))
        .map_err(err)?
        .collect::<Result<_, _>>()
        .map_err(err)?;
    for (name, ddl) in [
        ("source_path", "TEXT"),
        ("source_mtime", "INTEGER"),
        ("popularity", "REAL NOT NULL DEFAULT 0"),
    ] {
        if !columns.iter().any(|x| x == name) {
            db.execute_batch(&format!("ALTER TABLE documents ADD COLUMN {name} {ddl}"))
                .map_err(err)?;
        }
    }
    db.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_documents_source_path ON documents(source_path);
         CREATE INDEX IF NOT EXISTS idx_documents_category_source_path ON documents(category, source_path);",
    )
    .map_err(err)?;
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='documents_fts'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(err)?
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    let migrate = !sql.contains("content='documents'") && !sql.contains("content=\"documents\"");
    db.execute_batch("BEGIN IMMEDIATE;").map_err(err)?;
    let migration = (|| {
        if migrate {
            db.execute_batch("DROP TRIGGER IF EXISTS documents_fts_after_insert;DROP TRIGGER IF EXISTS documents_fts_after_delete;DROP TRIGGER IF EXISTS documents_fts_after_update;DROP TABLE IF EXISTS documents_fts;CREATE VIRTUAL TABLE documents_fts USING fts5(title,subtitle,tokens,content='documents',content_rowid='rowid',tokenize='unicode61');").map_err(err)?;
        }
        db.execute_batch("CREATE TRIGGER IF NOT EXISTS documents_fts_after_insert AFTER INSERT ON documents BEGIN INSERT INTO documents_fts(rowid,title,subtitle,tokens) VALUES(new.rowid,new.title,new.subtitle,new.tokens);END;
CREATE TRIGGER IF NOT EXISTS documents_fts_after_delete AFTER DELETE ON documents BEGIN INSERT INTO documents_fts(documents_fts,rowid,title,subtitle,tokens) VALUES('delete',old.rowid,old.title,old.subtitle,old.tokens);END;
CREATE TRIGGER IF NOT EXISTS documents_fts_after_update AFTER UPDATE ON documents BEGIN INSERT INTO documents_fts(documents_fts,rowid,title,subtitle,tokens) VALUES('delete',old.rowid,old.title,old.subtitle,old.tokens);INSERT INTO documents_fts(rowid,title,subtitle,tokens) VALUES(new.rowid,new.title,new.subtitle,new.tokens);END;").map_err(err)?;
        if migrate {
            db.execute_batch("INSERT INTO documents_fts(documents_fts) VALUES('rebuild');")
                .map_err(err)?;
        }
        Ok::<_, String>(())
    })();
    match migration {
        Ok(()) => db.execute_batch("COMMIT;").map_err(err)?,
        Err(e) => {
            let _ = db.execute_batch("ROLLBACK;");
            return Err(e);
        }
    }
    prune(&db)?;
    Ok(db)
}
fn prune(db: &Connection) -> Result<(), String> {
    for (table, n) in [
        ("click_events", 1000),
        ("action_usage_events", 5000),
        ("benchmark_snapshots", 50),
    ] {
        db.execute(&format!("DELETE FROM {table} WHERE id NOT IN(SELECT id FROM {table} ORDER BY id DESC LIMIT ? )"),[n]).map_err(err)?;
    }
    Ok(())
}
pub fn upsert(db: &Connection, d: &Value) -> Result<(), String> {
    let id = d["id"].as_str().ok_or("Document id required")?;
    let category = d["category"].as_str().ok_or("Document category required")?;
    // Only builders without a source modification time opt in. Their refresh
    // clock must not refresh ranking or rewrite FTS for an unchanged payload.
    let synthetic_time = d["_syntheticUpdatedAt"].as_bool().unwrap_or(false);
    db.execute("INSERT INTO documents(id,category,title,subtitle,tokens,action_json,updated_at,source_path,source_mtime,popularity) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(id) DO UPDATE SET category=excluded.category,title=excluded.title,subtitle=excluded.subtitle,tokens=excluded.tokens,action_json=excluded.action_json,updated_at=excluded.updated_at,source_path=excluded.source_path,source_mtime=excluded.source_mtime,popularity=excluded.popularity WHERE category<>excluded.category OR title<>excluded.title OR subtitle<>excluded.subtitle OR tokens<>excluded.tokens OR action_json<>excluded.action_json OR (NOT ?11 AND updated_at<>excluded.updated_at) OR popularity<>excluded.popularity OR source_path IS NOT excluded.source_path OR source_mtime IS NOT excluded.source_mtime",params![id,category,d["title"].as_str().unwrap_or(""),d["subtitle"].as_str().unwrap_or(""),d["tokens"].as_str().unwrap_or(""),serde_json::to_string(&d["action"]).map_err(err)?,d["updatedAt"].as_u64().unwrap_or(0),d["sourcePath"].as_str(),d["sourceMtime"].as_u64(),d["popularity"].as_f64().unwrap_or(0.),synthetic_time]).map_err(err)?;
    Ok(())
}
const SELECT:&str="SELECT d.id,d.category,d.title,d.subtitle,d.tokens,d.action_json,d.updated_at,d.popularity FROM documents d ";
fn row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let action: String = r.get(5)?;
    Ok(
        json!({"id":r.get::<_,String>(0)?,"category":r.get::<_,String>(1)?,"title":r.get::<_,String>(2)?,"subtitle":r.get::<_,String>(3)?,"tokens":r.get::<_,String>(4)?,"_actionJson":action,"updatedAt":r.get::<_,i64>(6)?,"popularity":r.get::<_,f64>(7)?}),
    )
}
fn collect(db: &Connection, sql: &str, args: impl rusqlite::Params) -> Result<Vec<Value>, String> {
    db.prepare(sql)
        .map_err(err)?
        .query_map(args, row)
        .map_err(err)?
        .collect::<Result<_, _>>()
        .map_err(err)
}
fn stat(db: &Connection, id: &str, q: &str) -> Result<(f64, f64, u64), String> {
    let sql = if q.is_empty() {
        "SELECT frequency,success_count,total_count,last_used_at FROM action_stats WHERE action_id=?1"
    } else {
        "SELECT frequency,success_count,total_count,last_used_at FROM query_action_stats WHERE action_id=?1 AND query=?2"
    };
    let mut stmt = db.prepare_cached(sql).map_err(err)?;
    let mut rows = if q.is_empty() {
        stmt.query(params![id])
    } else {
        stmt.query(params![id, q])
    }
    .map_err(err)?;
    if let Some(r) = rows.next().map_err(err)? {
        let total = r.get::<_, f64>(2).map_err(err)?;
        Ok((
            r.get(0).map_err(err)?,
            r.get::<_, f64>(1).map_err(err)? / total.max(1.),
            r.get(3).map_err(err)?,
        ))
    } else {
        Ok((0., 0., 0))
    }
}
fn best_match(text: &str, variants: &[String]) -> (f64, String) {
    variants
        .iter()
        .enumerate()
        .map(|(i, q)| {
            (
                text::lexical(text, q).max(text::fuzzy(text, q)) * if i == 0 { 1. } else { 0.9 },
                q.clone(),
            )
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((0., String::new()))
}
pub fn search(db: &Connection, q: &str, candidates: bool) -> CoreResult {
    let now = now_ms();
    let variants = text::variants(q);
    let normalized = text::normalize(q);
    let home = normalized.is_empty();
    let mut docs = HashMap::new();
    if home {
        let sql=format!("{SELECT} LEFT JOIN action_stats a ON a.action_id=d.id WHERE d.category<>'files' ORDER BY coalesce(a.last_used_at,0) DESC,updated_at DESC LIMIT 1200");
        for mut d in collect(db, &sql, [])? {
            d["_lex"] = json!(0.92);
            docs.insert(d["id"].as_str().unwrap().to_owned(), d);
        }
    } else {
        let query = text::fts(&variants.join(" "));
        if !query.is_empty() {
            let sql="SELECT d.id,d.category,d.title,d.subtitle,d.tokens,d.action_json,d.updated_at,d.popularity,bm25(documents_fts,5.0,2.0,1.0) FROM documents d JOIN documents_fts f ON f.rowid=d.rowid WHERE documents_fts MATCH ? ORDER BY bm25(documents_fts,5.0,2.0,1.0) LIMIT 160";
            let mut statement = db.prepare(sql).map_err(err)?;
            let rows = statement
                .query_map([query], |r| {
                    let mut d = row(r)?;
                    let bm25: f64 = r.get(8)?;
                    let inverse = if bm25.is_finite() {
                        let n = (-bm25).max(0.);
                        n / (1. + n)
                    } else {
                        0.5
                    };
                    let metadata = format!(
                        "{} {}",
                        d["title"].as_str().unwrap_or(""),
                        d["subtitle"].as_str().unwrap_or("")
                    );
                    d["_lex"] = json!(inverse.max(best_match(&metadata, &variants).0));
                    Ok(d)
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            for d in rows {
                docs.insert(d["id"].as_str().unwrap().to_owned(), d);
            }
        }
        if docs.len() < 160 {
            let rows = collect(
                db,
                &format!("{SELECT} WHERE category<>'files' ORDER BY updated_at DESC LIMIT 1600"),
                [],
            )?;
            let mut fuzzy = vec![];
            for mut d in rows {
                let content = format!(
                    "{} {} {}",
                    d["title"].as_str().unwrap_or(""),
                    d["subtitle"].as_str().unwrap_or(""),
                    d["tokens"].as_str().unwrap_or("")
                );
                let (lex, query) = best_match(&content, &variants);
                if lex <= 0. {
                    continue;
                }
                d["_lex"] = json!(lex);
                d["_distance"] = json!(text::distance(d["title"].as_str().unwrap_or(""), &query));
                fuzzy.push(d);
            }
            fuzzy.sort_by(|a, b| {
                b["_lex"]
                    .as_f64()
                    .unwrap_or(0.)
                    .total_cmp(&a["_lex"].as_f64().unwrap_or(0.))
            });
            fuzzy.truncate(160);
            for d in fuzzy {
                let id = d["id"].as_str().unwrap().to_owned();
                if docs.get(&id).map_or(true, |old| {
                    d["_lex"].as_f64().unwrap_or(0.) > old["_lex"].as_f64().unwrap_or(0.)
                }) {
                    docs.insert(id, d);
                }
            }
        }
        // Applications reserve a compact name/alias page even when files fill FTS.
        let mut applications = vec![];
        for mut d in collect(
            db,
            &format!("{SELECT} WHERE category='applications' LIMIT 2000"),
            [],
        )? {
            let title = d["title"].as_str().unwrap_or("");
            let lex = variants
                .iter()
                .enumerate()
                .map(|(i, q)| text::app_match(title, q) * if i == 0 { 1. } else { 0.9 })
                .fold(0., f64::max);
            if lex > 0. {
                d["_lex"] = json!(lex);
                d.as_object_mut().unwrap().remove("_distance");
                applications.push(d);
            }
        }
        applications.sort_by(|a, b| {
            b["_lex"]
                .as_f64()
                .unwrap_or(0.)
                .total_cmp(&a["_lex"].as_f64().unwrap_or(0.))
        });
        applications.truncate(20);
        for d in applications {
            docs.insert(d["id"].as_str().unwrap().to_owned(), d);
        }
        for mut d in collect(db,&format!("{SELECT} JOIN query_action_stats q ON q.action_id=d.id WHERE q.query=? AND q.success_count>0 ORDER BY q.last_used_at DESC LIMIT 20"),[&normalized])?{let id=d["id"].as_str().unwrap().to_owned();d["_lex"]=json!(0.);docs.entry(id).or_insert(d);}
    }
    let note_query = regex::Regex::new(r"(?i)\bnotes?\b|quick note")
        .unwrap()
        .is_match(&normalized);
    let mut out = vec![];
    for (_, mut d) in docs {
        let id = d["id"].as_str().unwrap_or("");
        let category = d["category"].as_str().unwrap_or("");
        let title = d["title"].as_str().unwrap_or("");
        let subtitle = d["subtitle"].as_str().unwrap_or("");
        let lex = d["_lex"].as_f64().unwrap_or(0.);
        let (freq, rate, last) = stat(db, id, if home { "" } else { &normalized })?;
        let age = now.saturating_sub(d["updatedAt"].as_u64().unwrap_or(0));
        let mut score = text::weighted(lex, age, category, d["popularity"].as_f64().unwrap_or(0.));
        if home {
            let recent: u64 = db
                .query_row(
                    "SELECT count(*) FROM action_usage_events WHERE action_id=? AND created_at>=?",
                    params![id, now.saturating_sub(300_000)],
                    |r| r.get(0),
                )
                .map_err(err)?;
            let activity = if last > 0 {
                last
            } else {
                d["updatedAt"].as_u64().unwrap_or(0)
            };
            let learned_category = [
                "applications",
                "extensions",
                "native-command",
                "commands",
                "quick-notes",
                "snippets",
                "quick-links",
            ]
            .contains(&category);
            let recommendation = if category == "extensions" {
                d["_actionJson"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .and_then(|a| a["launcher"]["recommendation"].as_f64())
                    .unwrap_or(0.)
                    .clamp(0., 2000.)
            } else {
                0.
            };
            score = text::weighted(0.92, now.saturating_sub(activity), category, 0.)
                + if freq > 0. {
                    ((freq + 1.).log10() / 2.).min(1.) * 100.
                } else {
                    0.
                }
                + rate.clamp(0., 1.) * 50.
                + if learned_category {
                    text::learned(freq, rate, last, now)
                } else {
                    0.
                }
                + text::hot(recent)
                + text::recommendation(id)
                + recommendation;
        } else {
            if let Some(distance) = d["_distance"].as_u64() {
                score += match distance {
                    0 => 80.,
                    1 => 50.,
                    2 => 20.,
                    _ => 0.,
                };
            }
            score += text::query_boost(freq, rate, last, now)
                + variants
                    .iter()
                    .enumerate()
                    .map(|(i, q)| {
                        (text::surface_boost(category, title, subtitle, q)
                            + text::exact_note(category, title, q, age)
                            + text::full_tokens(title, subtitle, q))
                            * if i == 0 { 1. } else { 0.9 }
                    })
                    .fold(0., f64::max)
                + text::note_recency(category, age)
                + if note_query { 780. } else { 120. };
        }
        d["score"] = json!(score.round());
        for key in ["tokens", "popularity", "_lex", "_distance"] {
            d.as_object_mut().unwrap().remove(key);
        }
        out.push(d);
    }
    out.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&a["score"].as_f64().unwrap_or(0.))
            .then_with(|| b["updatedAt"].as_u64().cmp(&a["updatedAt"].as_u64()))
    });
    out.truncate(if candidates { 1000 } else { 80 });
    for d in &mut out {
        let action = d
            .as_object_mut()
            .unwrap()
            .remove("_actionJson")
            .unwrap_or(Value::Null);
        d["action"] =
            serde_json::from_str::<Value>(action.as_str().unwrap_or("")).unwrap_or(Value::Null);
        d.as_object_mut().unwrap().remove("updatedAt");
    }
    Ok(json!(out))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn legacy_migration_keeps_usage_and_non_ascii() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.sqlite3");
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE documents(id TEXT PRIMARY KEY,category TEXT NOT NULL,title TEXT NOT NULL,subtitle TEXT NOT NULL,tokens TEXT NOT NULL,action_json TEXT NOT NULL,updated_at INTEGER NOT NULL);CREATE VIRTUAL TABLE documents_fts USING fts5(id UNINDEXED,title,subtitle,tokens);").unwrap();
        drop(c);
        let db = Database::new(path);
        db.upsert(vec![json!({"id":"app:chrome","category":"applications","title":"Google Chrome","subtitle":"Application","tokens":"chrome хром","action":{"type":"open-app","appName":"Google Chrome"},"updatedAt":1})],vec![]).await.unwrap();
        db.record("app:chrome".into(), json!({"query":"хром"}), true)
            .await
            .unwrap();
        let r = db.search("сркщьу".into(), false).await.unwrap();
        assert_eq!(r[0]["id"], "app:chrome");
        db.upsert(vec![], vec!["applications".into()])
            .await
            .unwrap();
        assert_eq!(
            db.call(|c| Ok(json!(c
                .query_row("SELECT frequency FROM action_stats", [], |r| r
                    .get::<_, i64>(0))
                .unwrap())))
                .await
                .unwrap(),
            1
        );
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[tokio::test]
    async fn refresh_preserves_unchanged_rows_and_removes_stale_documents() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("index.sqlite3"));
        let doc = json!({"id":"app:keep","category":"applications","title":"Keep","action":{"type":"open-app","appName":"Keep"},"updatedAt":1});
        let mut stale = doc.clone();
        stale["id"] = json!("app:stale");
        db.upsert(vec![doc.clone(), stale], vec![]).await.unwrap();
        db.call(|c| {
            c.execute_batch("CREATE TABLE refresh_changes(kind TEXT); CREATE TRIGGER refresh_deleted AFTER DELETE ON documents BEGIN INSERT INTO refresh_changes VALUES('delete'); END; CREATE TRIGGER refresh_updated AFTER UPDATE ON documents BEGIN INSERT INTO refresh_changes VALUES('update'); END;").map_err(err)?;
            Ok(Value::Null)
        }).await.unwrap();
        db.upsert(vec![doc.clone()], vec!["applications".into()])
            .await
            .unwrap();
        db.upsert(vec![doc], vec!["applications".into()])
            .await
            .unwrap();
        let changes = db
            .call(|c| {
                Ok(json!([
                    c.query_row(
                        "SELECT count(*) FROM refresh_changes WHERE kind='delete'",
                        [],
                        |r| r.get::<_, i64>(0)
                    )
                    .map_err(err)?,
                    c.query_row(
                        "SELECT count(*) FROM refresh_changes WHERE kind='update'",
                        [],
                        |r| r.get::<_, i64>(0)
                    )
                    .map_err(err)?
                ]))
            })
            .await
            .unwrap();
        assert_eq!(changes, json!([1, 0]));
        assert_eq!(
            db.search("keep".into(), false).await.unwrap()[0]["id"],
            "app:keep"
        );
        db.shutdown().await.unwrap();
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    #[ignore = "synthetic 100k-file search benchmark"]
    fn search_benchmark() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = init(dir.path().join("bench.sqlite3")).unwrap();
        let tx = db.transaction().unwrap();
        for n in 0..100_400 {
            let file = n < 100_000;
            upsert(&tx, &json!({"id":format!("doc:{n}"),"category":if file {"files"} else {"applications"},"title":if file {format!("Project document {n}.txt")} else {format!("Calculator {n}")},"subtitle":if file {format!("/projects/archive/{n}/document.txt")} else {format!("/Applications/Calculator {n}.app")},"tokens":"project archive document","action":{"type":"open","path":format!("/fixture/{n}")},"updatedAt":n})).unwrap();
        }
        tx.commit().unwrap();
        for query in ["calclator", ""] {
            for _ in 0..3 {
                search(&mut db, query, false).unwrap();
            }
            let start = std::time::Instant::now();
            for _ in 0..20 {
                search(&mut db, query, false).unwrap();
            }
            eprintln!(
                "search benchmark query={query:?} mean_ms={:.3}",
                start.elapsed().as_secs_f64() * 1000. / 20.
            );
        }
    }
}

#[cfg(test)]
mod reader_tests {
    use super::*;

    #[test]
    fn nonfile_queries_use_partial_index() {
        let dir = tempfile::tempdir().unwrap();
        let db = init(dir.path().join("index.sqlite3")).unwrap();
        for suffix in [
            "WHERE d.category<>'files' ORDER BY updated_at DESC LIMIT 1600",
            "LEFT JOIN action_stats a ON a.action_id=d.id WHERE d.category<>'files' ORDER BY coalesce(a.last_used_at,0) DESC,updated_at DESC LIMIT 1200",
        ] {
            let plan: Vec<String> = db.prepare(&format!("EXPLAIN QUERY PLAN {SELECT} {suffix}"))
                .unwrap().query_map([], |r| r.get(3)).unwrap().collect::<Result<_, _>>().unwrap();
            assert!(plan.iter().any(|line| line.contains("idx_documents_nonfile_updated")), "{plan:?}");
            eprintln!("nonfile query plan: {plan:?}");
        }
    }

    #[tokio::test]
    async fn reader_search_does_not_wait_for_writer_and_shutdown_joins_both() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("index.sqlite3"));
        db.upsert(vec![json!({"id":"app:calculator","category":"applications","title":"Calculator","action":{"type":"open-app"},"updatedAt":1})], vec![]).await.unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, released) = mpsc::channel();
        let writer_db = db.clone();
        let writer = tokio::spawn(async move {
            writer_db
                .call(move |c| {
                    let tx = c.transaction().map_err(err)?;
                    tx.execute("DELETE FROM documents", []).map_err(err)?;
                    let _ = started.send(());
                    released.recv().map_err(err)?;
                    tx.commit().map_err(err)?;
                    Ok(Value::Null)
                })
                .await
        });
        ready.await.unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            db.search("calclator".into(), false),
        )
        .await;
        // Always release the writer, including when the regression times out.
        release.send(()).unwrap();
        let rows = result.expect("search queued behind writer").unwrap();
        assert_eq!(rows[0]["id"], "app:calculator");
        assert_eq!(rows[0]["action"]["type"], "open-app");
        writer.await.unwrap().unwrap();
        assert_eq!(db.search("".into(), false).await.unwrap(), json!([]));
        db.shutdown().await.unwrap();
        db.shutdown().await.unwrap();
        assert!(db.threads.lock().unwrap().is_empty());
        assert!(db.search("".into(), false).await.is_err());
    }

    #[test]
    fn files_match_fts_but_are_not_fuzzy_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let db = init(dir.path().join("index.sqlite3")).unwrap();
        upsert(&db, &json!({"id":"file:calculator","category":"files","title":"Calculator","action":{"type":"open-file"}})).unwrap();
        assert_eq!(
            search(&db, "calculator", false).unwrap()[0]["id"],
            "file:calculator"
        );
        assert_eq!(search(&db, "calclator", false).unwrap(), json!([]));
    }
}

#[cfg(test)]
mod performance_regression_tests {
    use super::super::providers::{
        app_documents_at, command_documents_at, delete_watched_path, WATCHER_DELETE_SQL,
    };
    use super::*;

    fn observed_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db = init(dir.path().join("launcher.sqlite3")).unwrap();
        db.execute_batch("CREATE TABLE observed_updates(id TEXT); CREATE TRIGGER observed_update AFTER UPDATE ON documents BEGIN INSERT INTO observed_updates VALUES(new.id); END;").unwrap();
        (dir, db)
    }
    fn stamp(db: &Connection, id: &str) -> u64 {
        db.query_row("SELECT updated_at FROM documents WHERE id=?", [id], |r| {
            r.get(0)
        })
        .unwrap()
    }
    fn integrity(db: &Connection) {
        // rank=1 compares the external-content FTS index with the documents table.
        db.execute_batch(
            "INSERT INTO documents_fts(documents_fts,rank) VALUES('integrity-check',1);",
        )
        .unwrap();
    }
    fn refresh_docs(now: u64) -> Vec<Value> {
        let mut docs = app_documents_at(
            &[json!({"name":"Alpha","path":"/Applications/Alpha.app"})],
            now,
        );
        docs.extend(command_documents_at(
            &[json!({"id":"alpha","title":"Alpha native","subtitle":"Fixed","keywords":["alpha"]})],
            now,
        ));
        docs
    }
    #[test]
    fn real_builders_do_not_update_or_rewrite_fts_on_clock_only_refresh() {
        let (_dir, db) = observed_db();
        let initial = refresh_docs(1000);
        assert!(initial.iter().any(|d| d["category"] == "applications"));
        assert!(initial.iter().any(|d| d["category"] == "native-command"));
        assert!(initial.iter().any(|d| d["category"] == "commands"));
        for d in &initial {
            upsert(&db, d).unwrap();
        }
        let before = db.total_changes();
        for now in [11000, 61000, 121000] {
            for d in refresh_docs(now) {
                upsert(&db, &d).unwrap();
            }
        }
        let delta = db.total_changes() - before;
        let updates: u64 = db
            .query_row("SELECT count(*) FROM observed_updates", [], |r| r.get(0))
            .unwrap();
        println!("clock-only refresh: {} documents x 3 refreshes, {updates} UPDATEs, {delta} total_changes (includes FTS internals and observer)", initial.len());
        assert_eq!(
            delta, 0,
            "clock-only refresh must perform no document or FTS writes"
        );
        assert_eq!(updates, 0);
        for d in initial {
            assert_eq!(stamp(&db, d["id"].as_str().unwrap()), 1000);
        }
        integrity(&db);
    }
    #[test]
    fn real_builder_payload_changes_get_change_time_and_ranking_recency() {
        let (_dir, db) = observed_db();
        let apps = [json!({"name":"Alpha","path":"/Applications/Alpha.app"})];
        let id = "app:/Applications/Alpha.app";
        upsert(&db, &app_documents_at(&apps, 1000)[0]).unwrap();
        // Same category and score: timestamp is the actual ranking tie-breaker.
        let peer = json!({"id":"app:peer","category":"applications","title":"Peer","subtitle":"Application","tokens":"peer","action":{},"updatedAt":2000});
        upsert(&db, &peer).unwrap();
        assert_eq!(search(&db, "", true).unwrap()[0]["id"], "app:peer");
        upsert(&db, &app_documents_at(&apps, 3000)[0]).unwrap();
        assert_eq!(search(&db, "", true).unwrap()[0]["id"], "app:peer");
        let renamed = [json!({"name":"Beta","path":"/Applications/Alpha.app"})];
        let mut changed = app_documents_at(&renamed, 4000).remove(0);
        upsert(&db, &changed).unwrap();
        assert_eq!(stamp(&db, id), 4000);
        assert_eq!(search(&db, "", true).unwrap()[0]["id"], id);
        assert_eq!(
            db.query_row("SELECT title FROM documents WHERE id=?", [id], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Beta"
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM documents_fts WHERE documents_fts MATCH 'beta'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
            1
        );
        // Exercise every guard field independently, including action-only and category-only.
        for (i, (key, value)) in [
            (
                "action",
                json!({"type":"open-app","appName":"Beta","appPath":"/new-target"}),
            ),
            ("popularity", json!(123.0)),
            ("category", json!("commands")),
            ("subtitle", json!("New subtitle")),
            ("tokens", json!("beta newtoken")),
            ("sourcePath", json!("/new-source")),
            ("sourceMtime", json!(900)),
        ]
        .into_iter()
        .enumerate()
        {
            let now = 5000 + i as u64;
            changed[key] = value.clone();
            changed["updatedAt"] = json!(now);
            let before = db.total_changes();
            upsert(&db, &changed).unwrap();
            assert!(db.total_changes() > before, "{key} must update");
            assert_eq!(stamp(&db, id), now);
            let before = db.total_changes();
            changed["updatedAt"] = json!(now + 10000);
            upsert(&db, &changed).unwrap();
            assert_eq!(db.total_changes(), before, "{key} must then become stable");
        }
        let (category, action, popularity): (String, String, f64) = db
            .query_row(
                "SELECT category,action_json,popularity FROM documents WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(category, "commands");
        assert_eq!(
            serde_json::from_str::<Value>(&action).unwrap(),
            changed["action"]
        );
        assert_eq!(popularity, 123.0);
        // Native title changes also travel through the real command builder.
        for (title, now) in [("Before", 1000), ("After", 9000)] {
            for d in command_documents_at(&[json!({"id":"changing","title":title})], now) {
                upsert(&db, &d).unwrap();
            }
        }
        assert_eq!(stamp(&db, "native:changing"), 9000);
        integrity(&db);
    }
    #[test]
    fn source_timestamps_still_update_and_category_only_changes_are_not_lost() {
        let (_dir, db) = observed_db();
        for category in ["quick-notes", "snippets", "files", "extensions"] {
            let mut d = super::super::stores::document(
                category,
                category,
                "Real",
                "",
                "real",
                json!({}),
                1000,
            );
            upsert(&db, &d).unwrap();
            d["updatedAt"] = json!(2000);
            upsert(&db, &d).unwrap();
            assert_eq!(stamp(&db, category), 2000);
            d["category"] = json!("quick-links");
            upsert(&db, &d).unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT category FROM documents WHERE id=?",
                    [category],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "quick-links"
            );
            d["updatedAt"] = json!(1500);
            upsert(&db, &d).unwrap();
            assert_eq!(
                stamp(&db, category),
                1500,
                "source time remains authoritative even when restored backwards"
            );
            let before = db.total_changes();
            upsert(&db, &d).unwrap();
            assert_eq!(db.total_changes(), before);
        }
        integrity(&db);
    }
    fn file_doc(path: &str) -> Value {
        json!({"id":format!("file:{path}"),"category":"files","title":"Fixture","tokens":"fixture","action":{"path":path},"sourcePath":path,"updatedAt":1})
    }
    #[test]
    fn watcher_delete_keeps_exact_subtree_app_prefix_unicode_and_fts_semantics() {
        let (_dir, db) = observed_db();
        let sep = std::path::MAIN_SEPARATOR;
        let path = |parts: &[&str]| format!("{sep}{}", parts.join(&sep.to_string()));
        let foo = path(&["fixture", "foo"]);
        let child = path(&["fixture", "foo", "nested", "one.pdf"]);
        let sibling = path(&["fixture", "foobar", "keep.pdf"]);
        let unicode_dir = path(&["fixture", "Қазақ 日本"]);
        let unicode_child = format!("{unicode_dir}{sep}файл.txt");
        let app = json!({"id":"app:guard","category":"applications","title":"Application","tokens":"application","action":{},"sourcePath":child,"updatedAt":1});
        for p in [&foo, &child, &sibling, &unicode_child] {
            upsert(&db, &file_doc(p)).unwrap();
        }
        upsert(&db, &app).unwrap();
        let before = db.total_changes();
        assert_eq!(
            delete_watched_path(
                &db,
                &format!("file:{}", path(&["fixture", "missing.crdownload"]))
            )
            .unwrap(),
            0
        );
        assert_eq!(db.total_changes(), before);
        // Exact id still works even for legacy rows without source_path.
        let mut legacy = file_doc(&path(&["fixture", "legacy.pdf"]));
        legacy.as_object_mut().unwrap().remove("sourcePath");
        upsert(&db, &legacy).unwrap();
        assert_eq!(
            delete_watched_path(&db, legacy["id"].as_str().unwrap()).unwrap(),
            1
        );
        assert_eq!(delete_watched_path(&db, &format!("file:{foo}")).unwrap(), 2);
        assert_eq!(delete_watched_path(&db, &format!("file:{foo}")).unwrap(), 0);
        assert_eq!(stamp(&db, "app:guard"), 1);
        assert_eq!(stamp(&db, &format!("file:{sibling}")), 1);
        assert_eq!(
            delete_watched_path(&db, &format!("file:{unicode_dir}")).unwrap(),
            1
        );
        integrity(&db);
        // Rename replay: new path indexed, old path removed by the same watcher helper.
        let new = path(&["fixture", "renamed", "keep.pdf"]);
        upsert(&db, &file_doc(&new)).unwrap();
        assert_eq!(
            delete_watched_path(&db, &format!("file:{sibling}")).unwrap(),
            1
        );
        assert_eq!(stamp(&db, &format!("file:{new}")), 1);
        assert_eq!(delete_watched_path(&db, &format!("file:{new}")).unwrap(), 1);
        integrity(&db);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM documents_fts WHERE documents_fts MATCH 'fixture'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM documents_fts WHERE documents_fts MATCH 'application'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn watcher_composite_index_is_migrated_after_legacy_columns_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite3");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("CREATE TABLE documents(id TEXT PRIMARY KEY,category TEXT NOT NULL,title TEXT NOT NULL,subtitle TEXT NOT NULL,tokens TEXT NOT NULL,action_json TEXT NOT NULL,updated_at INTEGER NOT NULL);").unwrap();
        drop(old);
        for _ in 0..2 {
            let db = init(path.clone()).unwrap();
            let names = db
                .prepare("PRAGMA index_info(idx_documents_category_source_path)")
                .unwrap()
                .query_map([], |r| r.get::<_, String>(2))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(names, ["category", "source_path"]);
        }
    }
    #[test]
    fn watcher_plan_uses_bounded_composite_range_on_bundled_sqlite() {
        let (_dir, mut db) = observed_db();
        let tx = db.transaction().unwrap();
        for n in 0..3000 {
            upsert(&tx, &file_doc(&format!("/fixture/d{}/f{n}.txt", n % 30))).unwrap();
        }
        tx.commit().unwrap();
        let plan = |db: &Connection| {
            db.prepare(&format!("EXPLAIN QUERY PLAN {WATCHER_DELETE_SQL}"))
                .unwrap()
                .query_map(params!["file:/missing", "/missing/", "/missing0"], |r| {
                    r.get::<_, String>(3)
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join(" | ")
        };
        let steps = |db: &Connection| {
            let mut stmt = db.prepare(WATCHER_DELETE_SQL).unwrap();
            assert_eq!(
                stmt.execute(params!["file:/missing", "/missing/", "/missing0"])
                    .unwrap(),
                0
            );
            stmt.get_status(rusqlite::StatementStatus::VmStep)
        };
        let after_plan = plan(&db);
        let after = steps(&db);
        db.execute_batch("DROP INDEX IF EXISTS idx_documents_category_source_path")
            .unwrap();
        let before_plan = plan(&db);
        let before = steps(&db);
        println!("SQLite {}\nbefore: {before_plan}\nafter: {after_plan}\nmissing DELETE VM steps: {before} -> {after}", rusqlite::version());
        assert!(
            after_plan.contains("idx_documents_category_source_path"),
            "{after_plan}"
        );
        assert!(
            after_plan.contains("source_path>?") && after_plan.contains("source_path<?"),
            "{after_plan}"
        );
        assert!(after < 200, "no-op DELETE must be bounded: {after}");
        assert!(
            before > after * 10,
            "fixture must expose the prior category scan"
        );
        integrity(&db);
    }
}
