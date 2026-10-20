//! Legacy chat database, opened only on Tokio's blocking pool.
use super::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

fn open(path: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(path.parent().ok_or("Invalid chat database path")?).map_err(err)?;
    let db = Connection::open(path).map_err(err)?;
    db.busy_timeout(Duration::from_secs(10)).map_err(err)?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;
      CREATE TABLE IF NOT EXISTS chat_sessions(id TEXT PRIMARY KEY,title TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,working_directory TEXT);
      CREATE TABLE IF NOT EXISTS chat_turns(id TEXT PRIMARY KEY,session_id TEXT NOT NULL,role TEXT NOT NULL,text TEXT NOT NULL,response_meta_json TEXT,stages_json TEXT,timeline_json TEXT,attachments_json TEXT,error TEXT,created_at INTEGER NOT NULL,FOREIGN KEY(session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE);
      CREATE INDEX IF NOT EXISTS idx_chat_turns_session ON chat_turns(session_id);
      CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated ON chat_sessions(updated_at DESC);").map_err(err)?;
    for (table, col) in [
        ("chat_sessions", "working_directory"),
        ("chat_turns", "attachments_json"),
        ("chat_turns", "response_meta_json"),
        ("chat_turns", "timeline_json"),
        ("chat_turns", "stages_json"),
    ] {
        let mut q = db
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(err)?;
        let columns = q
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        if !columns.iter().any(|c| c == col) {
            db.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} TEXT"), [])
                .map_err(err)?;
        }
    }
    Ok(db)
}
fn session_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let mut v = json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"createdAt":r.get::<_,i64>(2)?,"updatedAt":r.get::<_,i64>(3)?});
    if let Some(cwd) = r.get::<_, Option<String>>(4)? {
        v["workingDirectory"] = json!(cwd);
    }
    Ok(v)
}
fn valid_stage(v: &Value) -> bool {
    v["index"].is_number()
        && v["label"].is_string()
        && matches!(s(&v["status"]), "running" | "done" | "failed")
}
fn normalize_extra(key: &str, v: &Value) -> Option<Value> {
    match key {
        "responseMeta"
            if v["provider"].is_string()
                && v["providerTitle"].is_string()
                && v["model"].is_string() =>
        {
            let mut result = json!({"provider":cut(s(&v["provider"]),80),"providerTitle":cut(s(&v["providerTitle"]),120),"model":cut(s(&v["model"]),160)});
            if let Some(n) = v["tokenCount"].as_f64() {
                result["tokenCount"] = json!(n.max(0.0).round());
            }
            Some(result)
        }
        "stages" | "timeline" | "attachments" => {
            let items = v.as_array()?;
            let result:Vec<Value>=items.iter().take(if key=="attachments" {4}else{usize::MAX}).filter_map(|v| {
                if key=="stages" {return valid_stage(v).then(||v.clone());}
                if key=="timeline" {return ((s(&v["type"])=="text" && !s(&v["text"]).is_empty()) || (s(&v["type"])=="stage" && valid_stage(&v["stage"]))).then(||v.clone());}
                if s(&v["kind"])!="image" || !v["name"].is_string() || !matches!(s(&v["mimeType"]),"image/png"|"image/jpeg"|"image/webp") {return None;}
                let mut a=json!({"kind":"image","name":cut(s(&v["name"]),120),"mimeType":v["mimeType"]});
                for k in ["width","height"] {if v[k].is_number(){a[k]=v[k].clone();}}
                Some(a)
            }).collect();
            (!result.is_empty()).then(|| json!(result))
        }
        _ => None,
    }
}
fn encoded(key: &str, v: &Value) -> Option<String> {
    normalize_extra(key, v).map(|v| v.to_string())
}
fn required<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v[k].as_str().ok_or_else(|| format!("Invalid {k}"))
}
fn timestamp(v: &Value, k: &str) -> Result<i64, String> {
    v[k].as_f64()
        .filter(|n| n.is_finite())
        .map(|n| n as i64)
        .ok_or_else(|| format!("Invalid {k}"))
}
fn operate(path: &Path, channel: &str, args: &[Value]) -> CoreResult {
    let mut db = open(path)?;
    let a = args.first().unwrap_or(&Value::Null);
    match channel {
        "chat:list" => {
            let limit = a
                .as_u64()
                .filter(|v| *v > 0)
                .unwrap_or(100)
                .min(i64::MAX as u64) as i64;
            let mut q=db.prepare("SELECT s.id,s.title,s.created_at,s.updated_at,s.working_directory,(SELECT COUNT(*) FROM chat_turns t WHERE t.session_id=s.id),(SELECT text FROM chat_turns t WHERE t.session_id=s.id AND t.role='user' ORDER BY created_at DESC LIMIT 1) FROM chat_sessions s ORDER BY updated_at DESC LIMIT ?").map_err(err)?;
            let rows = q
                .query_map([limit], |r| {
                    let mut v = session_row(r)?;
                    v["turnCount"] = json!(r.get::<_, i64>(5)?);
                    v["preview"] = json!(r.get::<_, Option<String>>(6)?.unwrap_or_default());
                    Ok(v)
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            Ok(json!(rows))
        }
        "chat:get" => {
            let id = a.as_str().ok_or("Session id must be a string")?;
            let row=db.query_row("SELECT id,title,created_at,updated_at,working_directory FROM chat_sessions WHERE id=?",[id],session_row).optional().map_err(err)?;
            let Some(mut result) = row else {
                return Ok(Value::Null);
            };
            let mut q=db.prepare("SELECT id,role,text,response_meta_json,stages_json,timeline_json,attachments_json,error,created_at FROM chat_turns WHERE session_id=? ORDER BY created_at ASC").map_err(err)?;
            let turns=q.query_map([id],|r|{
                let role=r.get::<_,String>(1)?;
                let mut v=json!({"id":r.get::<_,String>(0)?,"role":if role=="assistant"{"assistant"}else{"user"},"text":r.get::<_,String>(2)?,"createdAt":r.get::<_,i64>(8)?});
                for (index,key) in [(3,"responseMeta"),(4,"stages"),(5,"timeline"),(6,"attachments")] {
                    if let Some(raw)=r.get::<_,Option<String>>(index)? {if let Ok(parsed)=serde_json::from_str::<Value>(&raw){if let Some(extra)=normalize_extra(key,&parsed){v[key]=extra;}}}
                }
                if let Some(error)=r.get::<_,Option<String>>(7)?{v["error"]=json!(error);} Ok(v)
            }).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
            result["turns"] = json!(turns);
            Ok(result)
        }
        "chat:append" => {
            let session = &a["session"];
            let turn = &a["turn"];
            let sid = required(session, "id")?;
            let title = required(session, "title")?;
            let created = timestamp(session, "createdAt")?;
            let updated = timestamp(session, "updatedAt")?;
            if !session["workingDirectory"].is_null() && !session["workingDirectory"].is_string() {
                return Err("Invalid workingDirectory".into());
            }
            let tid = required(turn, "id")?;
            let role = required(turn, "role")?;
            if !matches!(role, "user" | "assistant") {
                return Err("Invalid turn role".into());
            }
            let text = required(turn, "text")?;
            let at = timestamp(turn, "createdAt")?;
            let tx = db.transaction().map_err(err)?;
            tx.execute("INSERT INTO chat_sessions(id,title,created_at,updated_at,working_directory) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET title=excluded.title,updated_at=excluded.updated_at,working_directory=excluded.working_directory",params![sid,title,created,updated,session["workingDirectory"].as_str()]).map_err(err)?;
            let owner = tx
                .query_row("SELECT session_id FROM chat_turns WHERE id=?", [tid], |r| {
                    r.get::<_, String>(0)
                })
                .optional()
                .map_err(err)?;
            if owner.is_some_and(|owner| owner != sid) {
                return Err("Turn belongs to a different session".into());
            }
            tx.execute("INSERT INTO chat_turns(id,session_id,role,text,response_meta_json,stages_json,timeline_json,attachments_json,error,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET text=excluded.text,response_meta_json=excluded.response_meta_json,stages_json=excluded.stages_json,timeline_json=excluded.timeline_json,attachments_json=excluded.attachments_json,error=excluded.error",params![tid,sid,role,text,encoded("responseMeta",&turn["responseMeta"]),encoded("stages",&turn["stages"]),encoded("timeline",&turn["timeline"]),encoded("attachments",&turn["attachments"]),turn["error"].as_str(),at]).map_err(err)?;
            tx.execute(
                "UPDATE chat_sessions SET updated_at=? WHERE id=?",
                params![at.max(now_ms() as i64), sid],
            )
            .map_err(err)?;
            tx.commit().map_err(err)?;
            Ok(json!({"ok":true}))
        }
        "chat:update-title" => {
            db.execute(
                "UPDATE chat_sessions SET title=?,updated_at=? WHERE id=?",
                params![required(a, "title")?, now_ms() as i64, required(a, "id")?],
            )
            .map_err(err)?;
            Ok(json!({"ok":true}))
        }
        "chat:delete-turn" => {
            let tx = db.transaction().map_err(err)?;
            let sid = required(a, "sessionId")?;
            let count = tx
                .execute(
                    "DELETE FROM chat_turns WHERE session_id=? AND id=?",
                    params![sid, required(a, "turnId")?],
                )
                .map_err(err)?;
            if count > 0 {
                tx.execute(
                    "UPDATE chat_sessions SET updated_at=? WHERE id=?",
                    params![now_ms() as i64, sid],
                )
                .map_err(err)?;
            }
            tx.commit().map_err(err)?;
            Ok(json!({"ok":count>0}))
        }
        "chat:delete" => {
            let n = db
                .execute(
                    "DELETE FROM chat_sessions WHERE id=?",
                    [a.as_str().ok_or("Invalid session id")?],
                )
                .map_err(err)?;
            Ok(json!({"ok":n>0}))
        }
        "chat:clear" => {
            db.execute_batch("BEGIN;DELETE FROM chat_turns;DELETE FROM chat_sessions;COMMIT;")
                .map_err(err)?;
            Ok(json!({"ok":true}))
        }
        _ => Err("Unknown chat storage channel".into()),
    }
}
pub async fn handle(ctx: &CoreContext, channel: &str, args: &[Value]) -> CoreResult {
    let path = ctx.paths.data_dir.join("chat/sessions.sqlite3");
    let channel = channel.to_owned();
    let args = args.to_vec();
    let wrap = matches!(
        channel.as_str(),
        "chat:append" | "chat:update-title" | "chat:delete-turn" | "chat:delete" | "chat:clear"
    );
    let result = tokio::task::spawn_blocking(move || operate(&path, &channel, &args))
        .await
        .map_err(err)?;
    if wrap {
        Ok(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    } else {
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrates_old_history_and_roundtrips_timeline_without_image_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.sqlite3");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE chat_sessions(id TEXT PRIMARY KEY,title TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);CREATE TABLE chat_turns(id TEXT PRIMARY KEY,session_id TEXT NOT NULL,role TEXT NOT NULL,text TEXT NOT NULL,stages_json TEXT,error TEXT,created_at INTEGER NOT NULL,FOREIGN KEY(session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE);INSERT INTO chat_sessions VALUES('old','Legacy',1,2);INSERT INTO chat_turns VALUES('old-turn','old','user','Original',NULL,NULL,1);").unwrap();
        drop(db);
        assert_eq!(
            operate(&path, "chat:get", &[json!("old")]).unwrap()["turns"][0]["text"],
            "Original"
        );
        let stage = json!({"index":0,"label":"read","status":"done","output":"Unicode 🦀"});
        let payload = json!({"session":{"id":"s","title":"Test","createdAt":1,"updatedAt":2,"workingDirectory":"/tmp"},"turn":{"id":"t","role":"assistant","text":"answer","createdAt":3,"stages":[stage],"timeline":[{"type":"text","text":"first"},{"type":"stage","stage":stage}],"attachments":[{"kind":"image","name":"screen","mimeType":"image/png","data":"secret"}],"responseMeta":{"provider":"test","providerTitle":"Test","model":"mock","tokenCount":3}}});
        operate(&path, "chat:append", &[payload]).unwrap();
        let got = operate(&path, "chat:get", &[json!("s")]).unwrap();
        assert_eq!(got["turns"][0]["timeline"][1]["stage"], stage);
        assert!(got["turns"][0]["attachments"][0].get("data").is_none());
        operate(&path, "chat:delete", &[json!("s")]).unwrap();
        let db = open(&path).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM chat_turns WHERE session_id='s'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
