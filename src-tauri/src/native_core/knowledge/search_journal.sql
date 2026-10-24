-- Connection-local, transactional invalidation. Rollback rolls these back too.
-- External connections cannot fire TEMP triggers; PRAGMA data_version fences them.
CREATE TEMP TABLE IF NOT EXISTS knowledge_search_generation(id INTEGER PRIMARY KEY, generation INTEGER NOT NULL);
INSERT OR IGNORE INTO knowledge_search_generation VALUES(1,0);
CREATE TEMP TABLE IF NOT EXISTS knowledge_search_dirty(rowid INTEGER PRIMARY KEY);
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_chunk_insert AFTER INSERT ON main.knowledge_chunks BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty VALUES(NEW.rowid);
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_chunk_delete BEFORE DELETE ON main.knowledge_chunks BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty VALUES(OLD.rowid);
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_chunk_update AFTER UPDATE OF rowid,id,source_id,embedding_json ON main.knowledge_chunks
WHEN OLD.rowid IS NOT NEW.rowid OR OLD.id IS NOT NEW.id OR OLD.source_id IS NOT NEW.source_id OR OLD.embedding_json IS NOT NEW.embedding_json BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty VALUES(OLD.rowid);
 INSERT OR IGNORE INTO knowledge_search_dirty VALUES(NEW.rowid);
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_embedding_insert AFTER INSERT ON main.knowledge_gemma_embeddings BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT rowid FROM knowledge_chunks WHERE id=NEW.chunk_id;
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_embedding_delete BEFORE DELETE ON main.knowledge_gemma_embeddings BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT rowid FROM knowledge_chunks WHERE id=OLD.chunk_id;
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_embedding_update AFTER UPDATE ON main.knowledge_gemma_embeddings
WHEN OLD.chunk_id IS NOT NEW.chunk_id OR OLD.model_key IS NOT NEW.model_key OR OLD.vector IS NOT NEW.vector BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT rowid FROM knowledge_chunks WHERE id IN (OLD.chunk_id,NEW.chunk_id);
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_source_insert AFTER INSERT ON main.knowledge_sources BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT rowid FROM knowledge_chunks WHERE source_id=NEW.id;
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_source_update AFTER UPDATE OF id,root_id,status ON main.knowledge_sources
WHEN OLD.id IS NOT NEW.id OR OLD.root_id IS NOT NEW.root_id OR OLD.status IS NOT NEW.status BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT rowid FROM knowledge_chunks WHERE source_id IN (OLD.id,NEW.id);
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_source_delete BEFORE DELETE ON main.knowledge_sources BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT rowid FROM knowledge_chunks WHERE source_id=OLD.id;
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_root_insert AFTER INSERT ON main.knowledge_roots BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT c.rowid FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id WHERE s.root_id=NEW.id;
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_root_update AFTER UPDATE OF id,enabled ON main.knowledge_roots
WHEN OLD.id IS NOT NEW.id OR OLD.enabled IS NOT NEW.enabled BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT c.rowid FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id WHERE s.root_id IN (OLD.id,NEW.id);
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
CREATE TEMP TRIGGER IF NOT EXISTS knowledge_search_root_delete BEFORE DELETE ON main.knowledge_roots BEGIN
 INSERT OR IGNORE INTO knowledge_search_dirty SELECT c.rowid FROM knowledge_chunks c JOIN knowledge_sources s ON s.id=c.source_id WHERE s.root_id=OLD.id;
 UPDATE knowledge_search_generation SET generation=generation+1;
END;
