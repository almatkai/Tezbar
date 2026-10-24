//! Startup-only migration of the authoritative learned embedding store.
//!
//! Call after schema.sql, before search_journal.sql, cache construction, or any
//! prepared embedding statements. Enable foreign_keys BEFORE opening an outer
//! transaction. No other connection may use this database during startup.
//! A private SAVEPOINT works both in autocommit mode and inside a caller-owned
//! transaction (including a rusqlite Transaction via &tx). Success never commits
//! the caller's transaction. Errors roll back this helper's entire schema/data
//! change; rollback errors are returned too. No version marker is trusted.
//!
//! Installed journal/custom dependencies are rejected before a table rebuild.
//! Already-v2 stores can be checked with an installed journal: no table is
//! replaced and invalidation triggers stay attached. No model identity, extracted
//! content, vector-index metadata, or settings are reset here.

use rusqlite::{Connection, OptionalExtension};
use std::sync::atomic::{AtomicU64, Ordering};

const TABLE: &str = "knowledge_gemma_embeddings";
const STAGING: &str = "knowledge_gemma_embeddings_schema_migration";
const INDEX: &str = "knowledge_gemma_model_chunks_idx";
const DELETE_TRIGGER: &str = "knowledge_gemma_chunk_delete";
const DELETE_SQL: &str = "CREATE TRIGGER knowledge_gemma_chunk_delete
    BEFORE DELETE ON knowledge_chunks BEGIN
    DELETE FROM knowledge_gemma_embeddings WHERE chunk_id = OLD.id; END";
const CREATE_SQL: &str = "CREATE TABLE knowledge_gemma_embeddings (
    chunk_id TEXT NOT NULL REFERENCES knowledge_chunks(id) ON DELETE CASCADE,
    model_key TEXT NOT NULL,
    vector BLOB NOT NULL,
    PRIMARY KEY(chunk_id, model_key)
)";
const INDEX_SQL: &str = "CREATE INDEX knowledge_gemma_model_chunks_idx
    ON knowledge_gemma_embeddings(model_key, chunk_id)";

type Result<T> = std::result::Result<T, String>;

fn sql_error(error: rusqlite::Error) -> String {
    format!("Embedding schema: {error}")
}

fn reject(reason: impl std::fmt::Display) -> String {
    format!("Embedding schema: {reason}; database left unchanged")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EmbeddingSchemaChange {
    Created,
    Migrated,
    /// The table was already v2; a missing covering index may have been restored.
    Unchanged,
}

pub(super) fn ensure_embedding_schema(db: &Connection) -> Result<EmbeddingSchemaChange> {
    ensure_with_hook(db, |_| Ok(()))
}

// Production always passes a no-op. Tests inject failure after table replacement,
// before releasing the helper's savepoint, using exactly the same migration.
fn ensure_with_hook(
    db: &Connection,
    after_replace: impl FnOnce(&Connection) -> Result<()>,
) -> Result<EmbeddingSchemaChange> {
    let mut savepoint = SchemaSavepoint::new(db)?;
    let work = (|| {
        if !db
            .pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))
            .map_err(sql_error)?
        {
            return Err(reject(
                "enable PRAGMA foreign_keys=ON before the startup transaction",
            ));
        }
        if db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name=?1 COLLATE NOCASE)",
                [TABLE],
                |r| r.get::<_, bool>(0),
            )
            .map_err(sql_error)?
        {
            return Err(reject("TEMP object shadows the embedding table"));
        }
        let definition: Option<(String, Option<String>)> = db
            .query_row(
                "SELECT type,sql FROM main.sqlite_schema WHERE name=?1 COLLATE NOCASE",
                [TABLE],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let shape = match definition {
            None => None,
            Some((kind, Some(sql))) if kind == "table" => Some(validate_table(db, &sql)?),
            _ => return Err(reject("expected an ordinary main embedding table")),
        };
        validate_indexes(db)?;
        if shape == Some(Shape::Composite) {
            ensure_index(db)?;
            return Ok(EmbeddingSchemaChange::Unchanged);
        }
        let chunk_delete = validate_dependencies(db)?;
        if shape.is_none() {
            db.execute_batch(CREATE_SQL).map_err(sql_error)?;
            ensure_index(db)?;
            after_replace(db)?;
            return Ok(EmbeddingSchemaChange::Created);
        }
        if db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name=?1 COLLATE NOCASE)",
                [STAGING],
                |r| r.get::<_, bool>(0),
            )
            .map_err(sql_error)?
        {
            return Err(reject("reserved migration staging object already exists"));
        }
        // Never rename the original: SQLite would rewrite dependent SQL/FKs.
        // Copy once to a new constrained table. Only the exact app-owned delete
        // trigger is temporarily removed, then restored from its original SQL.
        db.execute_batch(&CREATE_SQL.replacen(TABLE, STAGING, 1))
            .map_err(sql_error)?;
        db.execute_batch(
            "INSERT INTO main.knowledge_gemma_embeddings_schema_migration(chunk_id,model_key,vector)
             SELECT chunk_id,model_key,vector FROM main.knowledge_gemma_embeddings"
        ).map_err(sql_error)?;
        // A caller may have defer_foreign_keys enabled. A nested RELEASE would
        // not check those constraints; validate before dropping any original.
        if db
            .prepare("PRAGMA main.foreign_key_check(knowledge_gemma_embeddings_schema_migration)")
            .map_err(sql_error)?
            .exists([])
            .map_err(sql_error)?
        {
            return Err(reject(
                "legacy embedding rows contain orphan chunk references",
            ));
        }
        if chunk_delete.is_some() {
            db.execute_batch("DROP TRIGGER main.knowledge_gemma_chunk_delete")
                .map_err(sql_error)?;
        }
        db.execute_batch(
            "DROP TABLE main.knowledge_gemma_embeddings;
             ALTER TABLE main.knowledge_gemma_embeddings_schema_migration
               RENAME TO knowledge_gemma_embeddings;",
        )
        .map_err(sql_error)?;
        if let Some(sql) = chunk_delete {
            db.execute_batch(&sql).map_err(sql_error)?;
        }
        ensure_index(db)?;
        let sql: String = db
            .query_row(
                "SELECT sql FROM main.sqlite_schema WHERE name=?1",
                [TABLE],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        if validate_table(db, &sql)? != Shape::Composite {
            return Err(reject("replacement table did not validate"));
        }
        after_replace(db)?;
        Ok(EmbeddingSchemaChange::Migrated)
    })();
    match work {
        Ok(change) => match savepoint.release() {
            Ok(()) => Ok(change),
            Err(error) => Err(savepoint.rollback_error(error)),
        },
        Err(error) => Err(savepoint.rollback_error(error)),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Legacy,
    Composite,
}

// Accept known DDL only, not arbitrary CHECK/UNIQUE/COLLATE/default semantics
// that table_info cannot describe. Unknown shapes fail closed.
fn normalized(sql: &str) -> String {
    let mut out = String::new();
    let mut chars = sql.chars();
    while let Some(c) = chars.next() {
        if matches!(c, '"' | '\x60' | '[') {
            let end = if c == '[' { ']' } else { c };
            let mut identifier = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == end {
                    closed = true;
                    break;
                }
                identifier.push(next);
            }
            // Strip quoting only from simple identifiers. Whitespace or escaped
            // quotes inside an identifier must not normalize to a known name.
            if closed
                && !identifier.is_empty()
                && identifier
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            {
                out.push_str(&identifier);
            } else {
                out.push(c);
                out.push_str(&identifier);
                out.push(end);
            }
        } else if !c.is_ascii_whitespace() {
            out.push(c);
        }
    }
    out.trim_end_matches(';')
        .to_ascii_lowercase()
        .replacen("createtableifnotexists", "createtable", 1)
        .replacen("createtriggerifnotexists", "createtrigger", 1)
        .replacen("createindexifnotexists", "createindex", 1)
}

fn validate_table(db: &Connection, sql: &str) -> Result<Shape> {
    let definition = normalized(sql);
    let mut shape = None;
    for chunk in [
        "chunk_id TEXT PRIMARY KEY",
        "chunk_id TEXT NOT NULL PRIMARY KEY",
        "chunk_id TEXT PRIMARY KEY NOT NULL",
    ] {
        for (inline, tail) in [
            ("", ""),
            (" REFERENCES knowledge_chunks(id) ON DELETE CASCADE", ""),
            (
                "",
                ",FOREIGN KEY(chunk_id) REFERENCES knowledge_chunks(id) ON DELETE CASCADE",
            ),
        ] {
            let candidate = format!("CREATE TABLE {TABLE}({chunk}{inline},model_key TEXT NOT NULL,vector BLOB NOT NULL{tail})");
            if definition == normalized(&candidate) {
                shape = Some(Shape::Legacy);
            }
        }
    }
    for (inline, tail) in [
        (" REFERENCES knowledge_chunks(id) ON DELETE CASCADE", ""),
        (
            "",
            ",FOREIGN KEY(chunk_id) REFERENCES knowledge_chunks(id) ON DELETE CASCADE",
        ),
    ] {
        let candidate = format!("CREATE TABLE {TABLE}(chunk_id TEXT NOT NULL{inline},model_key TEXT NOT NULL,vector BLOB NOT NULL,PRIMARY KEY(chunk_id,model_key){tail})");
        if definition == normalized(&candidate) {
            shape = Some(Shape::Composite);
        }
    }
    let shape = shape.ok_or_else(|| reject("unsupported embedding table definition"))?;
    let columns = db
        .prepare("PRAGMA main.table_xinfo(knowledge_gemma_embeddings)")
        .map_err(sql_error)?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    if columns.len() != 3
        || columns
            .iter()
            .zip([
                ("chunk_id", "TEXT"),
                ("model_key", "TEXT"),
                ("vector", "BLOB"),
            ])
            .enumerate()
            .any(|(i, (c, (name, kind)))| {
                c.0 != name
                    || !c.1.eq_ignore_ascii_case(kind)
                    || (i > 0 || shape == Shape::Composite) && !c.2
                    || c.3.is_some()
                    || c.4
                        != if i == 0 {
                            1
                        } else if i == 1 && shape == Shape::Composite {
                            2
                        } else {
                            0
                        }
                    || c.5 != 0
            })
    {
        return Err(reject("unexpected embedding column/primary-key metadata"));
    }
    let foreign_keys = db
        .prepare("PRAGMA main.foreign_key_list(knowledge_gemma_embeddings)")
        .map_err(sql_error)?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    let cascade = vec![(
        "knowledge_chunks".into(),
        "chunk_id".into(),
        "id".into(),
        "NO ACTION".into(),
        "CASCADE".into(),
        "NONE".into(),
    )];
    if foreign_keys != cascade && !(shape == Shape::Legacy && foreign_keys.is_empty()) {
        return Err(reject("unexpected embedding foreign key metadata"));
    }
    Ok(shape)
}

fn validate_indexes(db: &Connection) -> Result<()> {
    let named: Option<(String, Option<String>)> = db
        .query_row(
            "SELECT tbl_name,sql FROM main.sqlite_schema WHERE name=?1 COLLATE NOCASE",
            [INDEX],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    if let Some((table, sql)) = named {
        if table != TABLE || sql.as_deref().map(normalized) != Some(normalized(INDEX_SQL)) {
            return Err(reject("incompatible model/chunk covering index"));
        }
    }
    let indexes = db
        .prepare("PRAGMA main.index_list(knowledge_gemma_embeddings)")
        .map_err(sql_error)?
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(3)?)))
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    for (name, origin) in indexes {
        if name != INDEX && origin != "pk" {
            return Err(reject(format!("unsupported embedding index {name}")));
        }
    }
    Ok(())
}

fn ensure_index(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE INDEX IF NOT EXISTS knowledge_gemma_model_chunks_idx
         ON knowledge_gemma_embeddings(model_key,chunk_id)",
    )
    .map_err(sql_error)
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn validate_dependencies(db: &Connection) -> Result<Option<String>> {
    let mut chunk_delete = None;
    for schema in ["main", "temp"] {
        let objects = db
            .prepare(&format!(
                "SELECT type,name,tbl_name,coalesce(sql,'') FROM {schema}.sqlite_schema"
            ))
            .map_err(sql_error)?
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        for (kind, name, table, sql) in objects {
            if schema == "temp"
                && kind == "trigger"
                && name.to_ascii_lowercase().starts_with("knowledge_search_")
            {
                return Err(reject(format!(
                    "journal trigger {name} is installed; migrate before search_journal.sql"
                )));
            }
            if (kind == "trigger" || kind == "view")
                && (table.eq_ignore_ascii_case(TABLE) || sql.to_ascii_lowercase().contains(TABLE))
            {
                if schema == "main"
                    && kind == "trigger"
                    && name == DELETE_TRIGGER
                    && normalized(&sql) == normalized(DELETE_SQL)
                {
                    chunk_delete = Some(sql);
                } else {
                    return Err(reject(format!(
                        "unsupported {schema} {kind} dependency {name}"
                    )));
                }
            }
            // Incoming FKs could cascade on DROP, even when the child table is
            // empty now. Never rewrite, disable, or silently drop them.
            if kind == "table" && !table.eq_ignore_ascii_case(TABLE) {
                let targets = db
                    .prepare(&format!(
                        "PRAGMA {schema}.foreign_key_list({})",
                        quote_identifier(&name)
                    ))
                    .map_err(sql_error)?
                    .query_map([], |r| r.get::<_, String>(2))
                    .map_err(sql_error)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(sql_error)?;
                if targets.iter().any(|t| t.eq_ignore_ascii_case(TABLE)) {
                    return Err(reject(format!("incoming foreign key from {schema}.{name}")));
                }
            }
        }
    }
    Ok(chunk_delete)
}

struct SchemaSavepoint<'a> {
    db: &'a Connection,
    name: String,
    active: bool,
}

impl<'a> SchemaSavepoint<'a> {
    fn new(db: &'a Connection) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "tezbar_embedding_schema_{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        db.execute_batch(&format!("SAVEPOINT {name}"))
            .map_err(sql_error)?;
        Ok(Self {
            db,
            name,
            active: true,
        })
    }
    fn release(&mut self) -> Result<()> {
        self.db
            .execute_batch(&format!("RELEASE SAVEPOINT {}", self.name))
            .map_err(sql_error)?;
        self.active = false;
        Ok(())
    }
    fn rollback_error(&mut self, error: String) -> String {
        match self.db.execute_batch(&format!(
            "ROLLBACK TO SAVEPOINT {}; RELEASE SAVEPOINT {}",
            self.name, self.name
        )) {
            Ok(()) => {
                self.active = false;
                error
            }
            Err(rollback) => format!("{error}; rollback failed: {rollback}"),
        }
    }
}

impl Drop for SchemaSavepoint<'_> {
    fn drop(&mut self) {
        if self.active {
            let _ = self.db.execute_batch(&format!(
                "ROLLBACK TO SAVEPOINT {}; RELEASE SAVEPOINT {}",
                self.name, self.name
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, types::Value};

    const Q8: &str =
        "google/embeddinggemma-2:bfcd298762cc34d0357ece5ebdd31791a3a374d8:q8:256:search-v1";
    const Q4: &str = "ngquocvinh/EmbeddingGemma-2-GGUF:8e83fab7c71a90d8f97ae7cacf6a488f654cb5b6:ea25550b050fa8826707c5e44659a7c18259606dcc6ba449f771e3c75cf4e663:q4_k_m:256:search-v1";

    fn fixture() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        // Keep the real pre-Q4 learned layout explicit as fresh schema evolves;
        // all parent tables, FTS and compatibility triggers remain production SQL.
        let legacy = include_str!("schema.sql").replace(
            "chunk_id TEXT NOT NULL REFERENCES knowledge_chunks(id) ON DELETE CASCADE,\n        model_key TEXT NOT NULL,\n        vector BLOB NOT NULL,\n        PRIMARY KEY(chunk_id, model_key)",
            "chunk_id TEXT PRIMARY KEY,\n        model_key TEXT NOT NULL,\n        vector BLOB NOT NULL");
        db.execute_batch(&legacy).unwrap();
        db.execute_batch(
            r#"INSERT INTO knowledge_stats(id) VALUES(1);
             INSERT INTO knowledge_roots VALUES('r','/fixture','deep','native',1,1,2);
             INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,status,total_pages,indexed_pages)
               VALUES('s','r','/fixture/file','hash',42,3,'indexed',1,1);
             INSERT INTO knowledge_pages(source_id,page_number,extracted_text,ocr_text,ocr_blocks_json,ocr_confidence)
               VALUES('s',1,'extracted','OCR','[]',0.9);
             INSERT INTO knowledge_chunks(id,source_id,page_number,text,embedding_json)
               VALUES('c','s',1,'text','[0.125,0.5]');
             INSERT INTO knowledge_images(id,source_id,ocr_text,embedding_json,width,height)
               VALUES('image','s','image OCR','[0.25]',10,20);
             INSERT INTO knowledge_metadata VALUES('settings','{"variant":"q8_0"}');
             INSERT INTO knowledge_metadata VALUES('checkpoint','{"cursor":42}');
             INSERT INTO knowledge_artifacts VALUES('a','hash','ocr','native','v1','settings',4);
             INSERT INTO knowledge_indexing_jobs VALUES(1,'job',2,1,0,5);
             INSERT INTO knowledge_indexing_queue VALUES('job',1,'r','/fixture/next');"#,
        ).unwrap();
        let builtin = vec![0.125_f64; 384];
        let json = format!("{:?}", builtin);
        db.execute(
            "UPDATE knowledge_chunks SET embedding_json=? WHERE id='c'",
            [json],
        )
        .unwrap();
        db
    }

    fn save(db: &Connection, chunk: &str, key: &str, vector: &[u8]) {
        db.execute(
            "INSERT OR REPLACE INTO knowledge_gemma_embeddings(chunk_id,model_key,vector) VALUES(?,?,?)",
            params![chunk, key, vector],
        ).unwrap();
    }

    fn embeddings(db: &Connection) -> Vec<(String, String, Vec<u8>)> {
        db.prepare("SELECT chunk_id,model_key,vector FROM knowledge_gemma_embeddings ORDER BY chunk_id,model_key")
            .unwrap().query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))
            .unwrap().collect::<rusqlite::Result<_>>().unwrap()
    }

    fn schema(db: &Connection) -> Vec<(String, String, String)> {
        db.prepare("SELECT type,name,coalesce(sql,'') FROM main.sqlite_schema ORDER BY type,name")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn other_rows(db: &Connection) -> Vec<(String, Vec<Vec<Value>>)> {
        let tables: Vec<String> = db.prepare("SELECT name FROM main.sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name!='knowledge_gemma_embeddings' ORDER BY name")
            .unwrap().query_map([], |r| r.get(0)).unwrap()
            .collect::<rusqlite::Result<_>>().unwrap();
        tables
            .into_iter()
            .map(|name| {
                let mut stmt = db
                    .prepare(&format!(
                        "SELECT * FROM \"{}\" ORDER BY 1",
                        name.replace('"', "\"\"")
                    ))
                    .unwrap();
                let columns = stmt.column_count();
                let rows = stmt
                    .query_map([], |r| {
                        (0..columns)
                            .map(|i| r.get(i))
                            .collect::<rusqlite::Result<Vec<Value>>>()
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap();
                (name, rows)
            })
            .collect()
    }

    #[test]
    fn legacy_fixture_proves_q4_overwrites_q8_before_migration() {
        let db = fixture();
        save(&db, "c", Q8, &[1, 2, 3]);
        save(&db, "c", Q4, &[4, 5, 6]);
        assert_eq!(
            embeddings(&db),
            vec![("c".into(), Q4.into(), vec![4, 5, 6])]
        );
    }

    #[test]
    fn migrated_store_retains_both_exact_variant_keys_and_vectors() {
        let db = fixture();
        let q8: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        save(&db, "c", Q8, &q8);
        let other = other_rows(&db);
        ensure_embedding_schema(&db).unwrap();
        save(&db, "c", Q4, &[0, 255, 0, 128]);
        assert_eq!(embeddings(&db).len(), 2);
        assert!(embeddings(&db).contains(&("c".into(), Q8.into(), q8)));
        assert_eq!(other_rows(&db), other);
        db.execute(
            "DELETE FROM knowledge_gemma_embeddings WHERE chunk_id='c' AND model_key=?",
            [Q4],
        )
        .unwrap();
        assert_eq!(embeddings(&db).len(), 1);
        assert_eq!(embeddings(&db)[0].1, Q8);
    }

    #[test]
    fn rerun_is_schema_noop_and_repairs_only_missing_covering_index() {
        let db = fixture();
        save(&db, "c", Q8, &[1, 0, 255]);
        assert_eq!(
            ensure_embedding_schema(&db).unwrap(),
            EmbeddingSchemaChange::Migrated
        );
        let before = schema(&db);
        let rows = embeddings(&db);
        let version: i64 = db
            .pragma_query_value(None, "schema_version", |r| r.get(0))
            .unwrap();
        for _ in 0..3 {
            assert_eq!(
                ensure_embedding_schema(&db).unwrap(),
                EmbeddingSchemaChange::Unchanged
            );
        }
        assert_eq!(schema(&db), before);
        assert_eq!(embeddings(&db), rows);
        assert_eq!(
            db.pragma_query_value(None, "schema_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            version
        );
        db.execute_batch("DROP INDEX knowledge_gemma_model_chunks_idx")
            .unwrap();
        ensure_embedding_schema(&db).unwrap();
        assert_eq!(schema(&db), before);
        let columns: Vec<String> = db
            .prepare("PRAGMA index_info(knowledge_gemma_model_chunks_idx)")
            .unwrap()
            .query_map([], |r| r.get(2))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(columns, ["model_key", "chunk_id"]);
    }

    #[test]
    fn fk_itself_cascades_both_models_and_rejects_orphans() {
        let db = fixture();
        ensure_embedding_schema(&db).unwrap();
        // Prove the FK, independently of the preserved compatibility trigger.
        db.execute_batch("DROP TRIGGER knowledge_gemma_chunk_delete")
            .unwrap();
        save(&db, "c", Q8, &[1]);
        save(&db, "c", Q4, &[2]);
        assert!(db
            .execute(
                "INSERT INTO knowledge_gemma_embeddings VALUES('missing',?,x'00')",
                [Q8]
            )
            .is_err());
        db.execute("DELETE FROM knowledge_chunks WHERE id='c'", [])
            .unwrap();
        assert!(embeddings(&db).is_empty());
    }

    #[test]
    fn fresh_database_uses_actual_composite_schema_without_migration() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        db.execute_batch(include_str!("schema.sql")).unwrap();
        assert_eq!(
            ensure_embedding_schema(&db).unwrap(),
            EmbeddingSchemaChange::Unchanged
        );
        assert!(embeddings(&db).is_empty());
        assert_eq!(
            ensure_embedding_schema(&db).unwrap(),
            EmbeddingSchemaChange::Unchanged
        );
    }

    #[test]
    fn absent_embedding_table_is_created_and_missing_parent_is_not_created() {
        let db = fixture();
        db.execute_batch("DROP TABLE knowledge_gemma_embeddings")
            .unwrap();
        let other = other_rows(&db);
        assert_eq!(
            ensure_embedding_schema(&db).unwrap(),
            EmbeddingSchemaChange::Created
        );
        assert_eq!(other_rows(&db), other);
        save(&db, "c", Q8, &[1]);
        save(&db, "c", Q4, &[2]);
        assert_eq!(embeddings(&db).len(), 2);

        // SQLite permits an empty referencing table before the parent exists.
        // The helper creates ONLY the learned table/index, never parent content.
        let empty = Connection::open_in_memory().unwrap();
        empty.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        assert_eq!(
            ensure_embedding_schema(&empty).unwrap(),
            EmbeddingSchemaChange::Created
        );
        assert_eq!(schema(&empty).iter().filter(|s| s.0 == "table").count(), 1);
    }

    #[test]
    fn nested_rusqlite_transaction_owns_commit_and_rollback() {
        let mut db = fixture();
        save(&db, "c", Q8, &[5, 0, 255]);
        let before = schema(&db);
        let rows = embeddings(&db);
        let other = other_rows(&db);
        {
            let tx = db.transaction().unwrap();
            ensure_embedding_schema(&tx).unwrap();
            save(&tx, "c", Q4, &[8]);
            assert!(!tx.is_autocommit());
            tx.rollback().unwrap();
        }
        assert_eq!(schema(&db), before);
        assert_eq!(embeddings(&db), rows);
        assert_eq!(other_rows(&db), other);
        let tx = db.transaction().unwrap();
        ensure_embedding_schema(&tx).unwrap();
        save(&tx, "c", Q4, &[9]);
        tx.commit().unwrap();
        assert_eq!(embeddings(&db).len(), 2);
    }

    #[test]
    fn injected_failure_after_replacement_rolls_back_every_change_only() {
        for nested in [false, true] {
            let mut db = fixture();
            save(&db, "c", Q8, &[9, 0, 2]);
            let before = schema(&db);
            let rows = embeddings(&db);
            let other = other_rows(&db);
            if nested {
                let tx = db.transaction().unwrap();
                tx.execute("INSERT INTO knowledge_metadata VALUES('caller','42')", [])
                    .unwrap();
                let error = ensure_with_hook(&tx, |_| Err("injected after replacement".into()))
                    .unwrap_err();
                assert_eq!(error, "injected after replacement");
                assert!(!tx.is_autocommit());
                assert_eq!(schema(&tx), before);
                assert_eq!(embeddings(&tx), rows);
                tx.commit().unwrap();
                assert_eq!(
                    db.query_row(
                        "SELECT value_json FROM knowledge_metadata WHERE key='caller'",
                        [],
                        |r| r.get::<_, String>(0)
                    )
                    .unwrap(),
                    "42"
                );
                db.execute("DELETE FROM knowledge_metadata WHERE key='caller'", [])
                    .unwrap();
            } else {
                assert!(
                    ensure_with_hook(&db, |_| Err("injected after replacement".into())).is_err()
                );
                assert!(db.is_autocommit());
            }
            assert_eq!(schema(&db), before);
            assert_eq!(embeddings(&db), rows);
            assert_eq!(other_rows(&db), other);
        }
    }

    #[test]
    fn panic_unwinds_private_savepoint_without_leaking_transaction() {
        let db = fixture();
        save(&db, "c", Q8, &[1, 2, 3]);
        let before = schema(&db);
        let rows = embeddings(&db);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ensure_with_hook(&db, |_| panic!("injected unwind"));
        }));
        assert!(result.is_err());
        assert!(db.is_autocommit());
        assert_eq!(schema(&db), before);
        assert_eq!(embeddings(&db), rows);
    }

    #[test]
    fn malformed_shapes_and_fake_version_marker_fail_without_change() {
        for ddl in [
            "CREATE TABLE knowledge_gemma_embeddings(chunk_id TEXT PRIMARY KEY,model_key TEXT NOT NULL,vector BLOB NOT NULL,extra TEXT)",
            "CREATE TABLE knowledge_gemma_embeddings(chunk_id TEXT PRIMARY KEY,model_key TEXT NOT NULL,vector TEXT NOT NULL)",
            "CREATE TABLE knowledge_gemma_embeddings(chunk_id TEXT PRIMARY KEY,model_key TEXT NOT NULL,vector BLOB NOT NULL CHECK(length(vector)>0))",
            "CREATE TABLE knowledge_gemma_embeddings(chunk_id TEXT NOT NULL,model_key TEXT NOT NULL,vector BLOB NOT NULL,PRIMARY KEY(model_key,chunk_id))",
            "CREATE TABLE knowledge_gemma_embeddings(chunk_id TEXT NOT NULL,model_key TEXT NOT NULL,vector BLOB NOT NULL,PRIMARY KEY(chunk_id,model_key))",
            "CREATE TABLE knowledge_gemma_embeddings(chunk_id TEXT NOT NULL REFERENCES knowledge_chunks(id) ON DELETE RESTRICT,model_key TEXT NOT NULL,vector BLOB NOT NULL,PRIMARY KEY(chunk_id,model_key))",
        ] {
            let db = fixture();
            db.execute_batch("DROP TABLE knowledge_gemma_embeddings").unwrap();
            db.execute_batch(ddl).unwrap();
            db.execute_batch("PRAGMA user_version=2").unwrap();
            save(&db,"c","unknown-old-key",&[0,255,128]);
            let before = schema(&db);
            let rows = embeddings(&db);
            let other = other_rows(&db);
            assert!(ensure_embedding_schema(&db).is_err(),"{ddl}");
            assert!(db.is_autocommit());
            assert_eq!(schema(&db),before);
            assert_eq!(embeddings(&db),rows);
            assert_eq!(other_rows(&db),other);
            assert_eq!(db.pragma_query_value(None,"user_version",|r|r.get::<_,i64>(0)).unwrap(),2);
        }
    }

    #[test]
    fn known_journal_precondition_rejects_legacy_without_losing_invalidation() {
        let db = fixture();
        save(&db, "c", Q8, &[1]);
        db.execute_batch(include_str!("search_journal.sql"))
            .unwrap();
        let before = schema(&db);
        let rows = embeddings(&db);
        let temp_before: String = db.query_row(
            "SELECT group_concat(sql,';') FROM (SELECT sql FROM temp.sqlite_schema ORDER BY name)",[],|r|r.get(0)
        ).unwrap();
        assert!(ensure_embedding_schema(&db)
            .unwrap_err()
            .contains("migrate before search_journal.sql"));
        assert_eq!(schema(&db), before);
        assert_eq!(embeddings(&db), rows);
        assert_eq!(db.query_row("SELECT group_concat(sql,';') FROM (SELECT sql FROM temp.sqlite_schema ORDER BY name)",[],|r|r.get::<_,String>(0)).unwrap(),temp_before);
        save(&db, "c", Q8, &[2]);
        assert_eq!(
            db.query_row(
                "SELECT generation FROM knowledge_search_generation WHERE id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM knowledge_search_dirty", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn migrate_then_journal_then_v2_noop_keeps_both_models_invalidation() {
        let db = fixture();
        ensure_embedding_schema(&db).unwrap();
        db.execute_batch(include_str!("search_journal.sql"))
            .unwrap();
        assert_eq!(
            ensure_embedding_schema(&db).unwrap(),
            EmbeddingSchemaChange::Unchanged
        );
        save(&db, "c", Q8, &[1]);
        save(&db, "c", Q4, &[2]);
        assert_eq!(
            db.query_row(
                "SELECT generation FROM knowledge_search_generation WHERE id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        db.execute(
            "DELETE FROM knowledge_gemma_embeddings WHERE model_key=?",
            [Q4],
        )
        .unwrap();
        assert_eq!(
            db.query_row(
                "SELECT generation FROM knowledge_search_generation WHERE id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            3
        );
        assert_eq!(embeddings(&db).len(), 1);
        let tx = db.unchecked_transaction().unwrap();
        save(&tx, "c", Q4, &[3]);
        tx.rollback().unwrap();
        assert_eq!(
            db.query_row(
                "SELECT generation FROM knowledge_search_generation WHERE id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            3
        );
    }

    #[test]
    fn unsupported_dependencies_are_rejected_unchanged() {
        for ddl in [
            "CREATE TRIGGER custom_embedding AFTER INSERT ON knowledge_gemma_embeddings BEGIN SELECT 1; END",
            "CREATE TEMP TRIGGER custom_embedding AFTER INSERT ON main.knowledge_gemma_embeddings BEGIN SELECT 1; END",
            "CREATE TRIGGER custom_reference AFTER UPDATE ON knowledge_chunks BEGIN DELETE FROM knowledge_gemma_embeddings WHERE chunk_id=OLD.id; END",
            "CREATE VIEW custom_embedding AS SELECT * FROM knowledge_gemma_embeddings",
            "CREATE TABLE dependent(chunk_id TEXT REFERENCES knowledge_gemma_embeddings(chunk_id) ON DELETE CASCADE); INSERT INTO dependent VALUES('c')",
            "CREATE UNIQUE INDEX extra_unique ON knowledge_gemma_embeddings(model_key)",
            "DROP INDEX knowledge_gemma_model_chunks_idx; CREATE INDEX knowledge_gemma_model_chunks_idx ON knowledge_gemma_embeddings(chunk_id,model_key)",
            "DROP TRIGGER knowledge_gemma_chunk_delete; CREATE TRIGGER knowledge_gemma_chunk_delete BEFORE DELETE ON knowledge_chunks BEGIN DELETE FROM knowledge_gemma_embeddings; END",
            "CREATE TABLE knowledge_gemma_embeddings_schema_migration(sentinel TEXT); INSERT INTO knowledge_gemma_embeddings_schema_migration VALUES('keep')",
            "CREATE TEMP TABLE knowledge_gemma_embeddings(sentinel TEXT)",
        ] {
            let db = fixture();
            save(&db,"c",Q8,&[1,255]);
            db.execute_batch(ddl).unwrap();
            let before = schema(&db);
            let other = other_rows(&db);
            // main qualification is necessary for the TEMP-shadow fixture.
            let rows: Vec<u8> = db.query_row("SELECT vector FROM main.knowledge_gemma_embeddings",[],|r|r.get(0)).unwrap();
            assert!(ensure_embedding_schema(&db).is_err(),"{ddl}");
            assert_eq!(schema(&db),before);
            assert_eq!(other_rows(&db),other);
            assert_eq!(db.query_row("SELECT vector FROM main.knowledge_gemma_embeddings",[],|r|r.get::<_,Vec<u8>>(0)).unwrap(),rows);
        }
    }

    #[test]
    fn legacy_invalid_rows_fail_copy_and_preserve_all_originals_even_deferred() {
        for deferred in [false, true] {
            for insert in [
                "INSERT INTO knowledge_gemma_embeddings VALUES(NULL,'key',x'00ff')",
                "INSERT INTO knowledge_gemma_embeddings VALUES('missing','key',x'00ff')",
            ] {
                let db = fixture();
                save(&db, "c", Q8, &[1]);
                db.execute_batch(insert).unwrap();
                let before = schema(&db);
                let count: i64 = db
                    .query_row("SELECT COUNT(*) FROM knowledge_gemma_embeddings", [], |r| {
                        r.get(0)
                    })
                    .unwrap();
                let tx = db.unchecked_transaction().unwrap();
                if deferred {
                    tx.execute_batch("PRAGMA defer_foreign_keys=ON").unwrap();
                }
                assert!(ensure_embedding_schema(&tx).is_err());
                assert_eq!(schema(&tx), before);
                assert_eq!(
                    tx.query_row("SELECT COUNT(*) FROM knowledge_gemma_embeddings", [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    count
                );
                tx.commit().unwrap();
                assert_eq!(schema(&db), before);
                assert_eq!(
                    db.query_row(
                        "SELECT hex(vector) FROM knowledge_gemma_embeddings WHERE model_key='key'",
                        [],
                        |r| r.get::<_, String>(0)
                    )
                    .unwrap(),
                    "00FF"
                );
            }
        }
    }

    #[test]
    fn foreign_key_precondition_does_not_change_connection_or_schema() {
        let db = fixture();
        db.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        let before = schema(&db);
        assert!(ensure_embedding_schema(&db)
            .unwrap_err()
            .contains("foreign_keys=ON"));
        assert_eq!(schema(&db), before);
        assert!(!db
            .pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))
            .unwrap());
        assert!(db.is_autocommit());
    }

    #[test]
    fn explicitly_declared_legacy_fk_is_preserved_as_cascade() {
        let db = fixture();
        db.execute_batch(
            "DROP TABLE knowledge_gemma_embeddings;
            CREATE TABLE knowledge_gemma_embeddings(
                chunk_id TEXT PRIMARY KEY REFERENCES knowledge_chunks(id) ON DELETE CASCADE,
                model_key TEXT NOT NULL,vector BLOB NOT NULL)",
        )
        .unwrap();
        save(&db, "c", Q8, &[0, 1, 255]);
        assert_eq!(
            ensure_embedding_schema(&db).unwrap(),
            EmbeddingSchemaChange::Migrated
        );
        assert_eq!(embeddings(&db)[0].2, [0, 1, 255]);
        save(&db, "c", Q4, &[2]);
        db.execute("DELETE FROM knowledge_chunks WHERE id='c'", [])
            .unwrap();
        assert!(embeddings(&db).is_empty());
    }

    #[test]
    fn thousand_rows_preserve_unknown_keys_blob_bytes_counts_and_every_other_table() {
        let db = fixture();
        for i in 0..1000 {
            let id = format!("fixture-{i:04}");
            db.execute("INSERT INTO knowledge_chunks(id,source_id,text,embedding_json) VALUES(?,'s','fixture','[0.5]')",[&id]).unwrap();
            let blob: Vec<u8> = (0..1024).map(|j| ((i + j) % 256) as u8).collect();
            let key = if i % 2 == 0 {
                Q8
            } else {
                "unknown/LEGACY:Exact-Key:768"
            };
            save(&db, &id, key, &blob);
        }
        let before = embeddings(&db);
        let other = other_rows(&db);
        ensure_embedding_schema(&db).unwrap();
        assert_eq!(embeddings(&db), before);
        assert_eq!(other_rows(&db), other);
        for i in 0..1000 {
            save(&db, &format!("fixture-{i:04}"), Q4, &[4, 0, 255]);
        }
        let after = embeddings(&db);
        assert_eq!(after.len(), 2000);
        for row in before {
            assert!(after.contains(&row));
        }
        assert_eq!(other_rows(&db), other);
    }

    #[test]
    fn file_database_reopen_retains_migrated_data_and_keys() {
        static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "tezbar-embedding-schema-{}-{}.sqlite",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        // Refuse to reuse an existing file. The harness sets TMPDIR to its scratch.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        struct RemoveFile(std::path::PathBuf);
        impl Drop for RemoveFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = RemoveFile(path.clone());
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
            db.execute_batch(include_str!("schema.sql")).unwrap();
            db.execute_batch("INSERT INTO knowledge_roots VALUES('r','/fixture','deep','native',1,1,2);
                INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,status)
                VALUES('s','r','/fixture/file','hash',1,1,'indexed');
                INSERT INTO knowledge_chunks(id,source_id,text) VALUES('c','s','persist')").unwrap();
            save(&db, "c", Q8, &[0, 1, 255, 0]);
            ensure_embedding_schema(&db).unwrap();
            save(&db, "c", Q4, &[4, 0, 128]);
        }
        let db = Connection::open(&path).unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        assert_eq!(
            ensure_embedding_schema(&db).unwrap(),
            EmbeddingSchemaChange::Unchanged
        );
        assert_eq!(embeddings(&db).len(), 2);
        assert!(embeddings(&db).contains(&("c".into(), Q8.into(), vec![0, 1, 255, 0])));
        assert!(embeddings(&db).contains(&("c".into(), Q4.into(), vec![4, 0, 128])));
    }

    #[test]
    fn partial_journal_installation_is_also_rejected_before_rebuild() {
        let db = fixture();
        db.execute_batch(
            "CREATE TEMP TRIGGER knowledge_search_chunk_insert AFTER INSERT ON main.knowledge_chunks
             BEGIN SELECT 1; END"
        ).unwrap();
        let before = schema(&db);
        assert!(ensure_embedding_schema(&db)
            .unwrap_err()
            .contains("migrate before search_journal.sql"));
        assert_eq!(schema(&db), before);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM temp.sqlite_schema WHERE name='knowledge_search_chunk_insert'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    }

    #[test]
    fn unrelated_schema_objects_and_exact_chunk_delete_sql_survive() {
        let db = fixture();
        db.execute_batch(
            "CREATE TABLE unrelated(sentinel TEXT);
             INSERT INTO unrelated VALUES('keep');
             CREATE TRIGGER unrelated_trigger AFTER INSERT ON unrelated BEGIN SELECT 1; END;
             CREATE VIEW unrelated_view AS SELECT sentinel FROM unrelated;
             CREATE INDEX unrelated_index ON unrelated(sentinel);",
        )
        .unwrap();
        let unaffected = |db: &Connection| {
            schema(db)
                .into_iter()
                .filter(|(_, name, _)| {
                    !name.contains("knowledge_gemma_embeddings")
                        && name != "knowledge_gemma_model_chunks_idx"
                })
                .collect::<Vec<_>>()
        };
        let before = unaffected(&db);
        let other = other_rows(&db);
        ensure_embedding_schema(&db).unwrap();
        assert_eq!(unaffected(&db), before);
        assert_eq!(other_rows(&db), other);
    }

    #[test]
    fn create_branch_fault_and_quoted_nonstandard_trigger_are_safe() {
        let db = fixture();
        db.execute_batch("DROP TABLE knowledge_gemma_embeddings")
            .unwrap();
        let before = schema(&db);
        assert!(ensure_with_hook(&db, |_| Err("injected create failure".into())).is_err());
        assert_eq!(schema(&db), before);
        assert!(db.is_autocommit());
        ensure_embedding_schema(&db).unwrap();

        let legacy = fixture();
        legacy
            .execute_batch(
                "DROP TRIGGER knowledge_gemma_chunk_delete;
             CREATE TRIGGER knowledge_gemma_chunk_delete BEFORE DELETE ON knowledge_chunks
             BEGIN DELETE FROM knowledge_gemma_embeddings WHERE \"chunk_ id\"=OLD.id; END",
            )
            .unwrap();
        let before = schema(&legacy);
        assert!(ensure_embedding_schema(&legacy).is_err());
        assert_eq!(schema(&legacy), before);
    }
}
