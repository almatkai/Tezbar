
      CREATE TABLE IF NOT EXISTS knowledge_roots (
        id TEXT PRIMARY KEY,
        path TEXT NOT NULL UNIQUE,
        depth TEXT NOT NULL,
        processing_backend TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS knowledge_sources (
        id TEXT PRIMARY KEY,
        root_id TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        content_hash TEXT NOT NULL,
        byte_size INTEGER NOT NULL,
        modified_at INTEGER NOT NULL,
        media_type TEXT,
        status TEXT NOT NULL,
        error TEXT,
        indexing_profile TEXT NOT NULL DEFAULT '',
        total_pages INTEGER,
        indexed_pages INTEGER NOT NULL DEFAULT 0,
        indexed_at INTEGER,
        FOREIGN KEY(root_id) REFERENCES knowledge_roots(id) ON DELETE CASCADE
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_sources_fts USING fts5(
        id UNINDEXED,
        path,
        tokenize = 'unicode61'
      );

      CREATE TRIGGER IF NOT EXISTS knowledge_sources_fts_insert
      AFTER INSERT ON knowledge_sources BEGIN
        INSERT INTO knowledge_sources_fts(rowid, id, path) VALUES (new.rowid, new.id, new.path);
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_sources_fts_delete
      AFTER DELETE ON knowledge_sources BEGIN
        DELETE FROM knowledge_sources_fts WHERE rowid = old.rowid;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_sources_fts_update
      AFTER UPDATE OF path ON knowledge_sources BEGIN
        UPDATE knowledge_sources_fts SET id = new.id, path = new.path WHERE rowid = old.rowid;
      END;

      CREATE TABLE IF NOT EXISTS knowledge_pages (
        source_id TEXT NOT NULL,
        page_number INTEGER NOT NULL,
        extracted_text TEXT,
        ocr_text TEXT,
        ocr_blocks_json TEXT,
        ocr_confidence REAL,
        extraction_status TEXT NOT NULL DEFAULT 'not-indexed',
        ocr_status TEXT NOT NULL DEFAULT 'not-indexed',
        embedding_status TEXT NOT NULL DEFAULT 'not-indexed',
        image_embedding_status TEXT NOT NULL DEFAULT 'not-indexed',
        content_hash TEXT,
        processing_cost_ms INTEGER,
        last_accessed_at INTEGER,
        PRIMARY KEY(source_id, page_number),
        FOREIGN KEY(source_id) REFERENCES knowledge_sources(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS knowledge_chunks (
        id TEXT PRIMARY KEY,
        source_id TEXT NOT NULL,
        page_number INTEGER,
        text TEXT NOT NULL,
        embedding_json TEXT,
        start_offset INTEGER,
        end_offset INTEGER,
        FOREIGN KEY(source_id) REFERENCES knowledge_sources(id) ON DELETE CASCADE
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_chunks_fts USING fts5(
        id UNINDEXED,
        source_id UNINDEXED,
        text,
        tokenize = 'unicode61'
      );

      CREATE TABLE IF NOT EXISTS knowledge_images (
        id TEXT PRIMARY KEY,
        source_id TEXT NOT NULL,
        page_number INTEGER,
        ocr_text TEXT,
        description TEXT,
        embedding_json TEXT,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        FOREIGN KEY(source_id) REFERENCES knowledge_sources(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS knowledge_artifacts (
        id TEXT PRIMARY KEY,
        source_hash TEXT NOT NULL,
        type TEXT NOT NULL,
        processor_id TEXT NOT NULL,
        processor_version TEXT NOT NULL,
        settings_hash TEXT NOT NULL,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS knowledge_metadata (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS knowledge_stats (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        source_count INTEGER NOT NULL DEFAULT 0,
        chunk_count INTEGER NOT NULL DEFAULT 0,
        indexed_page_count INTEGER NOT NULL DEFAULT 0,
        total_page_count INTEGER NOT NULL DEFAULT 0,
        partial_source_count INTEGER NOT NULL DEFAULT 0,
        source_bytes INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS knowledge_indexing_jobs (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        job_id TEXT NOT NULL,
        total_sources INTEGER NOT NULL,
        processed_sources INTEGER NOT NULL DEFAULT 0,
        failed_sources INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS knowledge_indexing_queue (
        job_id TEXT NOT NULL,
        position INTEGER NOT NULL,
        root_id TEXT NOT NULL,
        path TEXT NOT NULL,
        PRIMARY KEY(job_id, position)
      );

      CREATE INDEX IF NOT EXISTS knowledge_sources_root_idx ON knowledge_sources(root_id);
      CREATE INDEX IF NOT EXISTS knowledge_chunks_source_idx ON knowledge_chunks(source_id);
      CREATE INDEX IF NOT EXISTS knowledge_indexing_queue_job_idx
        ON knowledge_indexing_queue(job_id, position);


      CREATE TABLE IF NOT EXISTS knowledge_gemma_embeddings (
        chunk_id TEXT PRIMARY KEY,
        model_key TEXT NOT NULL,
        vector BLOB NOT NULL
      );
      CREATE INDEX IF NOT EXISTS knowledge_gemma_ready_chunks_idx
        ON knowledge_chunks(source_id, id) WHERE embedding_json IS NOT NULL;
      CREATE INDEX IF NOT EXISTS knowledge_gemma_model_chunks_idx
        ON knowledge_gemma_embeddings(model_key, chunk_id);
      CREATE TRIGGER IF NOT EXISTS knowledge_gemma_chunk_delete
      BEFORE DELETE ON knowledge_chunks BEGIN
        DELETE FROM knowledge_gemma_embeddings WHERE chunk_id = OLD.id;
      END;


      CREATE TRIGGER IF NOT EXISTS knowledge_stats_source_insert
      AFTER INSERT ON knowledge_sources
      BEGIN
        UPDATE knowledge_stats SET
          source_count = source_count + CASE WHEN NEW.status = 'indexed' THEN 1 ELSE 0 END,
          indexed_page_count = indexed_page_count + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.indexed_pages, 0) ELSE 0 END,
          total_page_count = total_page_count + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.total_pages, 0) ELSE 0 END,
          partial_source_count = partial_source_count + CASE WHEN NEW.status = 'indexed' AND COALESCE(NEW.total_pages, 0) > COALESCE(NEW.indexed_pages, 0) THEN 1 ELSE 0 END,
          source_bytes = source_bytes + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.byte_size, 0) ELSE 0 END
        WHERE id = 1;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_stats_source_delete
      AFTER DELETE ON knowledge_sources
      BEGIN
        UPDATE knowledge_stats SET
          source_count = MAX(0, source_count - CASE WHEN OLD.status = 'indexed' THEN 1 ELSE 0 END),
          indexed_page_count = MAX(0, indexed_page_count - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.indexed_pages, 0) ELSE 0 END),
          total_page_count = MAX(0, total_page_count - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.total_pages, 0) ELSE 0 END),
          partial_source_count = MAX(0, partial_source_count - CASE WHEN OLD.status = 'indexed' AND COALESCE(OLD.total_pages, 0) > COALESCE(OLD.indexed_pages, 0) THEN 1 ELSE 0 END),
          source_bytes = MAX(0, source_bytes - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.byte_size, 0) ELSE 0 END)
        WHERE id = 1;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_stats_source_update
      AFTER UPDATE OF status, indexed_pages, total_pages, byte_size ON knowledge_sources
      BEGIN
        UPDATE knowledge_stats SET
          source_count = MAX(0, source_count
            - CASE WHEN OLD.status = 'indexed' THEN 1 ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' THEN 1 ELSE 0 END),
          indexed_page_count = MAX(0, indexed_page_count
            - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.indexed_pages, 0) ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.indexed_pages, 0) ELSE 0 END),
          total_page_count = MAX(0, total_page_count
            - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.total_pages, 0) ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.total_pages, 0) ELSE 0 END),
          partial_source_count = MAX(0, partial_source_count
            - CASE WHEN OLD.status = 'indexed' AND COALESCE(OLD.total_pages, 0) > COALESCE(OLD.indexed_pages, 0) THEN 1 ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' AND COALESCE(NEW.total_pages, 0) > COALESCE(NEW.indexed_pages, 0) THEN 1 ELSE 0 END),
          source_bytes = MAX(0, source_bytes
            - CASE WHEN OLD.status = 'indexed' THEN COALESCE(OLD.byte_size, 0) ELSE 0 END
            + CASE WHEN NEW.status = 'indexed' THEN COALESCE(NEW.byte_size, 0) ELSE 0 END)
        WHERE id = 1;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_stats_chunk_insert
      AFTER INSERT ON knowledge_chunks
      BEGIN
        UPDATE knowledge_stats SET chunk_count = chunk_count + 1 WHERE id = 1;
      END;

      CREATE TRIGGER IF NOT EXISTS knowledge_stats_chunk_delete
      AFTER DELETE ON knowledge_chunks
      BEGIN
        UPDATE knowledge_stats SET chunk_count = MAX(0, chunk_count - 1) WHERE id = 1;
      END;
