import { app } from '@tezbar/desktop-runtime'
import DatabaseCtor, { type Database as DatabaseType } from 'better-sqlite3'
import { mkdirSync, statSync } from 'node:fs'
import { basename, join } from 'node:path'
import { createHash } from 'node:crypto'
import type {
  IndexingResult,
  KnowledgeRoot,
  KnowledgeReadResult,
  KnowledgeSearchHit,
  KnowledgeSettings,
  KnowledgeSourcesPage,
  KnowledgeStatus,
  SourceFingerprint,
} from '../../../shared/knowledge'
import { buildFtsQuery } from '../../search/textMatch'
import {
  cosineSimilarity,
  embedText,
  FEATURE_EMBEDDING_MODEL,
} from '../embeddings/featureEmbedding'
import { artifactSettingsHash } from '../core/fingerprint'
import { DEFAULT_KNOWLEDGE_SETTINGS } from '../depth'

type SourceRow = {
  id: string
  rootId: string
  path: string
  contentHash: string
  byteSize: number
  modifiedAt: number
  status: string
  error: string | null
  indexingProfile: string
}

type ChunkSearchRow = {
  id: string
  sourceId: string
  path: string
  pageNumber: number | null
  text: string
  embeddingJson: string | Uint8Array | null
  rank?: number
}

export type PersistedIndexingCandidate = {
  position: number
  rootId: string
  path: string
}

export type IndexingCheckpoint = {
  jobId: string
  totalSources: number
  processedSources: number
  failedSources: number
  candidates: PersistedIndexingCandidate[]
}

const DEFAULT_STATUS: KnowledgeStatus = {
  state: 'idle',
  backend: 'local',
  progress: 0,
  queuedSources: 0,
  processedSources: 0,
  failedSources: 0,
  sourceCount: 0,
  chunkCount: 0,
  indexedPageCount: 0,
  totalPageCount: 0,
  partialSourceCount: 0,
  sourceBytes: 0,
}

function databasePath(): string {
  const directory = join(app.getPath('userData'), 'knowledge')
  mkdirSync(directory, { recursive: true })
  return join(directory, 'knowledge.sqlite3')
}

function parseJson<T>(value: string | null | undefined, fallback: T): T {
  if (!value) return fallback
  try {
    return JSON.parse(value) as T
  } catch {
    return fallback
  }
}

function encodeEmbedding(value: readonly number[]): Buffer {
  const buffer = Buffer.allocUnsafe(value.length * Float32Array.BYTES_PER_ELEMENT)
  for (let index = 0; index < value.length; index += 1) {
    buffer.writeFloatLE(value[index] ?? 0, index * Float32Array.BYTES_PER_ELEMENT)
  }
  return buffer
}

function decodeEmbedding(
  value: string | Uint8Array | null | undefined,
): number[] | undefined {
  if (!value) return undefined
  if (typeof value === 'string') return parseJson<number[] | undefined>(value, undefined)
  const buffer = Buffer.isBuffer(value)
    ? value
    : Buffer.from(value.buffer, value.byteOffset, value.byteLength)
  if (buffer.byteLength % Float32Array.BYTES_PER_ELEMENT !== 0) return undefined
  const result = new Array<number>(buffer.byteLength / Float32Array.BYTES_PER_ELEMENT)
  for (let index = 0; index < result.length; index += 1) {
    result[index] = buffer.readFloatLE(index * Float32Array.BYTES_PER_ELEMENT)
  }
  return result
}

let singleton: KnowledgeStore | null = null

export function getKnowledgeStore(): KnowledgeStore {
  singleton ??= new KnowledgeStore()
  return singleton
}

export class KnowledgeStore {
  private database: DatabaseType | null = null

  private get db(): DatabaseType {
    if (!this.database) throw new Error('Knowledge database is not initialized')
    return this.database
  }

  ensureInitialized(): void {
    if (this.database) return
    this.database = new DatabaseCtor(databasePath())
    this.db.pragma('journal_mode = WAL')
    this.db.pragma('synchronous = NORMAL')
    this.db.pragma('busy_timeout = 5000')
    this.db.exec(`
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
    `)
    this.ensureColumn('knowledge_sources', 'indexing_profile', "TEXT NOT NULL DEFAULT ''")
    this.ensureColumn('knowledge_sources', 'total_pages', 'INTEGER')
    this.ensureColumn('knowledge_sources', 'indexed_pages', 'INTEGER NOT NULL DEFAULT 0')
    this.db.exec(`
      CREATE INDEX IF NOT EXISTS knowledge_sources_reuse_idx
        ON knowledge_sources(content_hash, indexing_profile, status, indexed_at DESC)
    `)
    this.ensureColumn('knowledge_pages', 'extraction_status', "TEXT NOT NULL DEFAULT 'not-indexed'")
    this.ensureColumn('knowledge_pages', 'ocr_status', "TEXT NOT NULL DEFAULT 'not-indexed'")
    this.ensureColumn('knowledge_pages', 'embedding_status', "TEXT NOT NULL DEFAULT 'not-indexed'")
    this.ensureColumn(
      'knowledge_pages',
      'image_embedding_status',
      "TEXT NOT NULL DEFAULT 'not-indexed'"
    )
    this.ensureColumn('knowledge_pages', 'content_hash', 'TEXT')
    this.ensureColumn('knowledge_pages', 'processing_cost_ms', 'INTEGER')
    this.ensureColumn('knowledge_pages', 'last_accessed_at', 'INTEGER')
    this.db
      .prepare(
        `
      UPDATE knowledge_roots SET depth = 'inherit'
      WHERE depth IN ('text', 'text-and-images')
    `
      )
      .run()
    this.ensureMaterializedStats()
  }

  /**
   * Keep dashboard counters in one tiny row instead of repeatedly scanning the
   * multi-gigabyte chunks table. The previous COUNT(*) calls ran on every
   * indexing progress update and could monopolize the backend for minutes.
   *
   * Existing installs are seeded from the already-persisted status snapshot,
   * so this migration itself never performs a full-table scan. Triggers keep
   * the counters exact for all subsequent source/chunk mutations.
   */
  private ensureMaterializedStats(): void {
    const existing = this.db.prepare('SELECT id FROM knowledge_stats WHERE id = 1').get() as
      | { id: number }
      | undefined
    if (!existing) {
      const row = this.db
        .prepare("SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'status'")
        .get() as { valueJson: string } | undefined
      const status = parseJson<Partial<KnowledgeStatus>>(row?.valueJson, {})
      const count = (value: unknown): number => {
        const numeric = typeof value === 'number' && Number.isFinite(value) ? value : 0
        return Math.max(0, Math.round(numeric))
      }
      this.db
        .prepare(
          `
        INSERT INTO knowledge_stats
          (id, source_count, chunk_count, indexed_page_count, total_page_count,
           partial_source_count, source_bytes)
        VALUES (1, ?, ?, ?, ?, ?, ?)
      `
        )
        .run(
          count(status.sourceCount),
          count(status.chunkCount),
          count(status.indexedPageCount),
          count(status.totalPageCount),
          count(status.partialSourceCount),
          count(status.sourceBytes)
        )
    }

    this.db.exec(`
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
    `)
  }

  private ensureColumn(table: string, column: string, definition: string): void {
    const columns = this.db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>
    if (columns.some((candidate) => candidate.name === column)) return
    this.db.exec(`ALTER TABLE ${table} ADD COLUMN ${column} ${definition}`)
  }

  listRoots(): KnowledgeRoot[] {
    this.ensureInitialized()
    const rows = this.db
      .prepare(
        `
      SELECT id, path, depth, processing_backend AS processingBackend,
             enabled, created_at AS createdAt, updated_at AS updatedAt
      FROM knowledge_roots ORDER BY created_at ASC
    `
      )
      .all() as Array<Omit<KnowledgeRoot, 'enabled'> & { enabled: number }>
    return rows.map((row) => ({ ...row, enabled: row.enabled === 1 }))
  }

  upsertRoot(root: KnowledgeRoot): void {
    this.ensureInitialized()
    this.db
      .prepare(
        `
      INSERT INTO knowledge_roots
        (id, path, depth, processing_backend, enabled, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        path = excluded.path,
        depth = excluded.depth,
        processing_backend = excluded.processing_backend,
        enabled = excluded.enabled,
        updated_at = excluded.updated_at
    `
      )
      .run(
        root.id,
        root.path,
        root.depth,
        root.processingBackend,
        root.enabled ? 1 : 0,
        root.createdAt,
        root.updatedAt
      )
  }

  removeRoot(rootId: string): void {
    this.ensureInitialized()
    const sourceIds = this.db
      .prepare('SELECT id FROM knowledge_sources WHERE root_id = ?')
      .all(rootId) as Array<{ id: string }>
    const transaction = this.db.transaction(() => {
      for (const source of sourceIds) this.removeSource(source.id)
      this.db.prepare('DELETE FROM knowledge_roots WHERE id = ?').run(rootId)
    })
    transaction()
  }

  getSourceByPath(path: string): SourceRow | null {
    this.ensureInitialized()
    return (
      (this.db
        .prepare(
          `
      SELECT id, root_id AS rootId, path, content_hash AS contentHash,
             byte_size AS byteSize, modified_at AS modifiedAt, status, error,
             indexing_profile AS indexingProfile
      FROM knowledge_sources WHERE path = ?
    `
        )
        .get(path) as SourceRow | undefined) ?? null
    )
  }

  markSourcePending(input: {
    id: string
    rootId: string
    path: string
    fingerprint: SourceFingerprint
    mediaType?: string
    indexingProfile: string
  }): void {
    this.ensureInitialized()
    this.db
      .prepare(
        `
      INSERT INTO knowledge_sources
        (id, root_id, path, content_hash, byte_size, modified_at, media_type, status, error,
         indexing_profile)
      VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?)
      ON CONFLICT(id) DO UPDATE SET
        root_id = excluded.root_id,
        path = excluded.path,
        content_hash = excluded.content_hash,
        byte_size = excluded.byte_size,
        modified_at = excluded.modified_at,
        media_type = excluded.media_type,
        indexing_profile = excluded.indexing_profile,
        status = 'pending',
        error = NULL
    `
      )
      .run(
        input.id,
        input.rootId,
        input.path,
        input.fingerprint.contentHash,
        input.fingerprint.byteSize,
        input.fingerprint.modifiedAt,
        input.mediaType ?? null,
        input.indexingProfile
      )
  }

  markSourceFailed(sourceId: string, error: string): void {
    this.ensureInitialized()
    this.db
      .prepare(
        `
      UPDATE knowledge_sources SET status = 'failed', error = ?, indexed_at = ? WHERE id = ?
    `
      )
      .run(error.slice(0, 2_000), Date.now(), sourceId)
  }

  saveResult(
    rootId: string,
    path: string,
    fingerprint: SourceFingerprint,
    indexingProfile: string,
    result: IndexingResult
  ): void {
    this.ensureInitialized()
    const deleteFts = this.db.prepare(`
      DELETE FROM knowledge_chunks_fts
      WHERE rowid IN (SELECT rowid FROM knowledge_chunks WHERE source_id = ?)
        AND source_id = ?
    `)
    const embeddedPages = new Set(
      result.chunks.filter((chunk) => chunk.embedding).map((chunk) => chunk.pageNumber ?? 1)
    )
    const imageEmbeddedPages = new Set(
      result.images.filter((image) => image.embedding).map((image) => image.pageNumber ?? 1)
    )
    const transaction = this.db.transaction(() => {
      this.db
        .prepare(
          `
        INSERT INTO knowledge_sources
          (id, root_id, path, content_hash, byte_size, modified_at, status, error,
           indexing_profile, total_pages, indexed_pages, indexed_at)
        VALUES (?, ?, ?, ?, ?, ?, 'indexed', NULL, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          root_id = excluded.root_id,
          path = excluded.path,
          content_hash = excluded.content_hash,
          byte_size = excluded.byte_size,
          modified_at = excluded.modified_at,
          indexing_profile = excluded.indexing_profile,
          total_pages = excluded.total_pages,
          indexed_pages = excluded.indexed_pages,
          status = 'indexed',
          error = NULL,
          indexed_at = excluded.indexed_at
      `
        )
        .run(
          result.sourceId,
          rootId,
          path,
          fingerprint.contentHash,
          fingerprint.byteSize,
          fingerprint.modifiedAt,
          indexingProfile,
          result.totalPageCount ?? result.pages.length,
          result.pages.length,
          Date.now()
        )

      // FTS5 cannot index the UNINDEXED source_id metadata column. Chunk and
      // FTS rows are deliberately assigned the same rowid, allowing a fast
      // lookup through knowledge_chunks_source_idx instead of scanning the
      // entire full-text index for every indexed file.
      deleteFts.run(result.sourceId, result.sourceId)
      this.db.prepare('DELETE FROM knowledge_pages WHERE source_id = ?').run(result.sourceId)
      this.db.prepare('DELETE FROM knowledge_chunks WHERE source_id = ?').run(result.sourceId)
      this.db.prepare('DELETE FROM knowledge_images WHERE source_id = ?').run(result.sourceId)

      const insertPage = this.db.prepare(`
        INSERT INTO knowledge_pages
          (source_id, page_number, extracted_text, ocr_text, ocr_blocks_json, ocr_confidence,
           extraction_status, ocr_status, embedding_status, image_embedding_status,
           content_hash, processing_cost_ms, last_accessed_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)
      `)
      for (const page of result.pages) {
        const pageText = `${page.extractedText ?? ''}\n${page.ocr?.text ?? ''}`.trim()
        insertPage.run(
          result.sourceId,
          page.pageNumber,
          page.extractedText ?? null,
          page.ocr?.text ?? null,
          page.ocr ? JSON.stringify(page.ocr.blocks) : null,
          page.ocr?.averageConfidence ?? null,
          page.extractedText ? 'text-indexed' : 'metadata-only',
          page.ocr ? 'ocr-indexed' : 'not-indexed',
          embeddedPages.has(page.pageNumber) ? 'embedded' : 'not-indexed',
          imageEmbeddedPages.has(page.pageNumber) ? 'embedded' : 'not-indexed',
          pageText ? createHash('sha256').update(pageText).digest('hex') : null
        )
      }

      const insertChunk = this.db.prepare(`
        INSERT INTO knowledge_chunks
          (id, source_id, page_number, text, embedding_json, start_offset, end_offset)
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `)
      const insertChunkFts = this.db.prepare(`
        INSERT INTO knowledge_chunks_fts (rowid, id, source_id, text) VALUES (?, ?, ?, ?)
      `)
      for (const chunk of result.chunks) {
        const inserted = insertChunk.run(
          chunk.id,
          result.sourceId,
          chunk.pageNumber ?? null,
          chunk.text,
          chunk.embedding ? encodeEmbedding(chunk.embedding) : null,
          chunk.startOffset ?? null,
          chunk.endOffset ?? null
        )
        insertChunkFts.run(inserted.lastInsertRowid, chunk.id, result.sourceId, chunk.text)
      }

      const insertImage = this.db.prepare(`
        INSERT INTO knowledge_images
          (id, source_id, page_number, ocr_text, description, embedding_json, width, height)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `)
      for (const image of result.images) {
        insertImage.run(
          image.id,
          result.sourceId,
          image.pageNumber ?? null,
          image.ocrText ?? null,
          image.description ?? null,
          image.embedding ? encodeEmbedding(image.embedding) : null,
          image.width,
          image.height
        )
      }

      const insertArtifact = this.db.prepare(`
        INSERT OR IGNORE INTO knowledge_artifacts
          (id, source_hash, type, processor_id, processor_version, settings_hash, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `)
      for (const capability of result.completedCapabilities) {
        const processor =
          capability === 'text-embeddings' && result.textEmbeddingModel
            ? result.textEmbeddingModel
            : result.extractor
        const settingsHash = artifactSettingsHash({ capability, processor })
        const artifactId = createHash('sha256')
          .update(
            `${result.sourceHash}:${capability}:${processor.id}:${processor.version}:${settingsHash}`
          )
          .digest('hex')
        insertArtifact.run(
          artifactId,
          result.sourceHash,
          capability,
          processor.id,
          processor.version,
          settingsHash,
          Date.now()
        )
      }
    })
    transaction()
  }

  findReusableResult(
    sourceHash: string,
    targetSourceId: string,
    indexingProfile: string
  ): IndexingResult | null {
    this.ensureInitialized()
    const source = this.db
      .prepare(
        `
      SELECT id, total_pages AS totalPages FROM knowledge_sources
      WHERE content_hash = ? AND indexing_profile = ? AND status = 'indexed' AND id <> ?
      ORDER BY indexed_at DESC LIMIT 1
    `
      )
      .get(sourceHash, indexingProfile, targetSourceId) as
      | { id: string; totalPages: number | null }
      | undefined
    if (!source) return null

    const pages = this.db
      .prepare(
        `
      SELECT page_number AS pageNumber, extracted_text AS extractedText,
             ocr_text AS ocrText, ocr_blocks_json AS ocrBlocksJson,
             ocr_confidence AS ocrConfidence
      FROM knowledge_pages WHERE source_id = ? ORDER BY page_number ASC
    `
      )
      .all(source.id) as Array<{
      pageNumber: number
      extractedText: string | null
      ocrText: string | null
      ocrBlocksJson: string | null
      ocrConfidence: number | null
    }>
    const chunks = this.db
      .prepare(
        `
      SELECT id, page_number AS pageNumber, text, embedding_json AS embeddingJson,
             start_offset AS startOffset, end_offset AS endOffset
      FROM knowledge_chunks WHERE source_id = ? ORDER BY rowid ASC
    `
      )
      .all(source.id) as Array<{
      id: string
      pageNumber: number | null
      text: string
      embeddingJson: string | Uint8Array | null
      startOffset: number | null
      endOffset: number | null
    }>
    const images = this.db
      .prepare(
        `
      SELECT id, page_number AS pageNumber, ocr_text AS ocrText, description,
             embedding_json AS embeddingJson, width, height
      FROM knowledge_images WHERE source_id = ? ORDER BY rowid ASC
    `
      )
      .all(source.id) as Array<{
      id: string
      pageNumber: number | null
      ocrText: string | null
      description: string | null
      embeddingJson: string | Uint8Array | null
      width: number
      height: number
    }>

    const hasOcr =
      pages.some((page) => Boolean(page.ocrText)) || images.some((image) => Boolean(image.ocrText))
    const hasEmbeddings = chunks.some((chunk) => Boolean(chunk.embeddingJson))
    return {
      sourceId: targetSourceId,
      sourceHash,
      extractor: { id: 'content-addressed-cache', version: '1.0.0' },
      textEmbeddingModel: hasEmbeddings ? FEATURE_EMBEDDING_MODEL : undefined,
      pages: pages.map((page) => ({
        pageNumber: page.pageNumber,
        extractedText: page.extractedText ?? undefined,
        ocr: page.ocrText
          ? {
              text: page.ocrText,
              blocks: parseJson(page.ocrBlocksJson, []),
              averageConfidence: page.ocrConfidence ?? undefined,
            }
          : undefined,
      })),
      chunks: chunks.map((chunk) => ({
        id: createHash('sha256').update(`${targetSourceId}:${chunk.id}`).digest('hex'),
        pageNumber: chunk.pageNumber ?? undefined,
        text: chunk.text,
        embedding: decodeEmbedding(chunk.embeddingJson),
        startOffset: chunk.startOffset ?? undefined,
        endOffset: chunk.endOffset ?? undefined,
      })),
      images: images.map((image) => ({
        id: createHash('sha256').update(`${targetSourceId}:${image.id}`).digest('hex'),
        pageNumber: image.pageNumber ?? undefined,
        ocrText: image.ocrText ?? undefined,
        description: image.description ?? undefined,
        embedding: decodeEmbedding(image.embeddingJson),
        width: image.width,
        height: image.height,
      })),
      completedCapabilities: [
        'extracted-text',
        ...(hasOcr ? ['ocr' as const] : []),
        'chunks',
        ...(hasEmbeddings ? ['text-embeddings' as const] : []),
      ],
      warnings: ['Reused locally cached artifacts for identical content.'],
      totalPageCount: source.totalPages ?? pages.length,
    }
  }

  removeSource(sourceId: string): void {
    this.ensureInitialized()
    this.db
      .prepare(`
        DELETE FROM knowledge_chunks_fts
        WHERE rowid IN (SELECT rowid FROM knowledge_chunks WHERE source_id = ?)
          AND source_id = ?
      `)
      .run(sourceId, sourceId)
    this.db.prepare('DELETE FROM knowledge_pages WHERE source_id = ?').run(sourceId)
    this.db.prepare('DELETE FROM knowledge_chunks WHERE source_id = ?').run(sourceId)
    this.db.prepare('DELETE FROM knowledge_images WHERE source_id = ?').run(sourceId)
    this.db.prepare('DELETE FROM knowledge_sources WHERE id = ?').run(sourceId)
  }

  removeMissingSources(rootId: string, presentPaths: ReadonlySet<string>): number {
    this.ensureInitialized()
    const rows = this.db
      .prepare('SELECT id, path FROM knowledge_sources WHERE root_id = ?')
      .all(rootId) as Array<{ id: string; path: string }>
    let removed = 0
    for (const row of rows) {
      if (presentPaths.has(row.path)) continue
      this.removeSource(row.id)
      removed += 1
    }
    return removed
  }

  counts(): {
    sourceCount: number
    chunkCount: number
    indexedPageCount: number
    totalPageCount: number
    partialSourceCount: number
    sourceBytes: number
  } {
    this.ensureInitialized()
    const stats = this.db
      .prepare(
        `
      SELECT source_count AS sourceCount,
             chunk_count AS chunkCount,
             indexed_page_count AS indexedPageCount,
             total_page_count AS totalPageCount,
             partial_source_count AS partialSourceCount,
             source_bytes AS sourceBytes
      FROM knowledge_stats WHERE id = 1
    `
      )
      .get() as {
      sourceCount: number
      chunkCount: number
      indexedPageCount: number
      totalPageCount: number
      partialSourceCount: number
      sourceBytes: number
    }
    return stats
  }

  storageBytes(): number {
    this.ensureInitialized()
    const path = databasePath()
    return [path, `${path}-wal`, `${path}-shm`].reduce((total, candidate) => {
      try {
        return total + statSync(candidate).size
      } catch {
        return total
      }
    }, 0)
  }

  listSources(input?: { query?: string; offset?: number; limit?: number }): KnowledgeSourcesPage {
    this.ensureInitialized()
    const query = input?.query?.trim() ?? ''
    const offset = Math.max(0, Math.round(input?.offset ?? 0))
    const limit = Math.max(1, Math.min(500, Math.round(input?.limit ?? 200)))
    const where = query ? 'WHERE path LIKE ?' : ''
    const parameters = query ? [`%${query}%`] : []
    const total = this.db
      .prepare(
        `
      SELECT COUNT(*) AS count FROM knowledge_sources ${where}
    `
      )
      .get(...parameters) as { count: number }
    const rows = this.db
      .prepare(
        `
      SELECT id, path, byte_size AS byteSize, modified_at AS modifiedAt,
             indexed_at AS indexedAt, status, error,
             COALESCE(total_pages, 0) AS totalPageCount,
             COALESCE(indexed_pages, 0) AS indexedPageCount
      FROM knowledge_sources ${where}
      ORDER BY CASE status WHEN 'pending' THEN 0 WHEN 'failed' THEN 1 ELSE 2 END,
               indexed_at DESC, path ASC
      LIMIT ? OFFSET ?
    `
      )
      .all(...parameters, limit, offset) as Array<{
      id: string
      path: string
      byteSize: number
      modifiedAt: number
      indexedAt: number | null
      status: 'pending' | 'indexed' | 'failed'
      error: string | null
      totalPageCount: number
      indexedPageCount: number
    }>
    return {
      sources: rows.map((row) => ({
        ...row,
        title: basename(row.path),
        indexedAt: row.indexedAt ?? undefined,
        error: row.error ?? undefined,
      })),
      total: total.count,
      offset,
      hasMore: offset + rows.length < total.count,
    }
  }

  getPersistedStatus(): KnowledgeStatus {
    this.ensureInitialized()
    const row = this.db
      .prepare("SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'status'")
      .get() as { valueJson: string } | undefined
    const counts = this.counts()
    return { ...DEFAULT_STATUS, ...parseJson(row?.valueJson, {}), ...counts }
  }

  saveStatus(status: KnowledgeStatus): void {
    this.ensureInitialized()
    this.db
      .prepare(
        `
      INSERT INTO knowledge_metadata (key, value_json) VALUES ('status', ?)
      ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
    `
      )
      .run(JSON.stringify(status))
  }

  saveIndexingCheckpoint(
    jobId: string,
    candidates: ReadonlyArray<{ rootId: string; path: string }>
  ): IndexingCheckpoint {
    this.ensureInitialized()
    const insertCandidate = this.db.prepare(
      `
      INSERT INTO knowledge_indexing_queue (job_id, position, root_id, path)
      VALUES (?, ?, ?, ?)
    `
    )
    const transaction = this.db.transaction(() => {
      this.db.prepare('DELETE FROM knowledge_indexing_queue').run()
      this.db.prepare('DELETE FROM knowledge_indexing_jobs').run()
      this.db
        .prepare(
          `
        INSERT INTO knowledge_indexing_jobs
          (id, job_id, total_sources, processed_sources, failed_sources, created_at)
        VALUES (1, ?, ?, 0, 0, ?)
      `
        )
        .run(jobId, candidates.length, Date.now())
      candidates.forEach((candidate, position) => {
        insertCandidate.run(jobId, position, candidate.rootId, candidate.path)
      })
    })
    transaction()
    return this.getIndexingCheckpoint()!
  }

  getIndexingCheckpoint(): IndexingCheckpoint | null {
    this.ensureInitialized()
    const job = this.db
      .prepare(
        `
      SELECT job_id AS jobId, total_sources AS totalSources,
             processed_sources AS processedSources, failed_sources AS failedSources
      FROM knowledge_indexing_jobs WHERE id = 1
    `
      )
      .get() as Omit<IndexingCheckpoint, 'candidates'> | undefined
    if (!job) return null
    const candidates = this.db
      .prepare(
        `
      SELECT position, root_id AS rootId, path
      FROM knowledge_indexing_queue
      WHERE job_id = ?
      ORDER BY position ASC
    `
      )
      .all(job.jobId) as PersistedIndexingCandidate[]
    return { ...job, candidates }
  }

  completeIndexingCandidate(jobId: string, position: number, failed: boolean): boolean {
    this.ensureInitialized()
    let completed = false
    const transaction = this.db.transaction(() => {
      const removed = this.db
        .prepare('DELETE FROM knowledge_indexing_queue WHERE job_id = ? AND position = ?')
        .run(jobId, position)
      if (removed.changes === 0) return
      this.db
        .prepare(
          `
        UPDATE knowledge_indexing_jobs
        SET processed_sources = processed_sources + 1,
            failed_sources = failed_sources + ?
        WHERE id = 1 AND job_id = ?
      `
        )
        .run(failed ? 1 : 0, jobId)
      completed = true
    })
    transaction()
    return completed
  }

  clearIndexingCheckpoint(jobId?: string): void {
    this.ensureInitialized()
    const transaction = this.db.transaction(() => {
      if (jobId) {
        this.db.prepare('DELETE FROM knowledge_indexing_queue WHERE job_id = ?').run(jobId)
        this.db
          .prepare('DELETE FROM knowledge_indexing_jobs WHERE id = 1 AND job_id = ?')
          .run(jobId)
        return
      }
      this.db.prepare('DELETE FROM knowledge_indexing_queue').run()
      this.db.prepare('DELETE FROM knowledge_indexing_jobs').run()
    })
    transaction()
  }

  getSettings(): KnowledgeSettings {
    this.ensureInitialized()
    const row = this.db
      .prepare("SELECT value_json AS valueJson FROM knowledge_metadata WHERE key = 'settings'")
      .get() as { valueJson: string } | undefined
    const persisted = parseJson<Partial<KnowledgeSettings>>(row?.valueJson, {})
    const depth = ['off', 'basic', 'smart', 'deep'].includes(persisted.depth ?? '')
      ? (persisted.depth as KnowledgeSettings['depth'])
      : DEFAULT_KNOWLEDGE_SETTINGS.depth
    return { ...DEFAULT_KNOWLEDGE_SETTINGS, ...persisted, depth }
  }

  saveSettings(settings: KnowledgeSettings): void {
    this.ensureInitialized()
    this.db
      .prepare(
        `
      INSERT INTO knowledge_metadata (key, value_json) VALUES ('settings', ?)
      ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
    `
      )
      .run(JSON.stringify(settings))
  }

  search(query: string, limit = 12, rootIds?: string[]): KnowledgeSearchHit[] {
    this.ensureInitialized()
    const trimmed = query.trim()
    if (!trimmed || limit <= 0 || rootIds?.length === 0) return []
    const rootFilter = rootIds ? `AND r.id IN (${rootIds.map(() => '?').join(', ')})` : ''
    const ftsQuery = buildFtsQuery(trimmed)
    const lexicalRows = ftsQuery
      ? (this.db
          .prepare(
            `
          SELECT c.id, c.source_id AS sourceId, s.path, c.page_number AS pageNumber,
                 c.text, c.embedding_json AS embeddingJson,
                 bm25(knowledge_chunks_fts, 1.0) AS rank
          FROM knowledge_chunks_fts f
          JOIN knowledge_chunks c ON c.id = f.id
          JOIN knowledge_sources s ON s.id = c.source_id
          JOIN knowledge_roots r ON r.id = s.root_id
          WHERE knowledge_chunks_fts MATCH ? AND r.enabled = 1 ${rootFilter}
          ORDER BY rank ASC LIMIT ?
        `
          )
          .all(ftsQuery, ...(rootIds ?? []), Math.max(40, limit * 5)) as ChunkSearchRow[])
      : []

    const semanticRows = this.db
      .prepare(
        `
      SELECT c.id, c.source_id AS sourceId, s.path, c.page_number AS pageNumber,
             c.text, c.embedding_json AS embeddingJson
      FROM knowledge_chunks c
      JOIN knowledge_sources s ON s.id = c.source_id
      JOIN knowledge_roots r ON r.id = s.root_id
      WHERE c.embedding_json IS NOT NULL AND s.status = 'indexed' AND r.enabled = 1
      ${rootFilter}
      -- Chunks are replaced when a source is re-indexed, so rowid gives us
      -- the same useful recency bias without sorting the entire chunk table.
      -- Ordering by sources.indexed_at made every search scan and sort all
      -- indexed chunks (roughly two seconds on a large knowledge database).
      ORDER BY c.rowid DESC LIMIT 1200
    `
      )
      .all(...(rootIds ?? [])) as ChunkSearchRow[]

    const queryEmbedding = embedText(trimmed)
    const byId = new Map<string, KnowledgeSearchHit>()
    const lexicalIds = new Set(lexicalRows.map((row) => row.id))
    const candidates = new Map<string, ChunkSearchRow>()
    for (const row of [...lexicalRows, ...semanticRows]) candidates.set(row.id, row)

    for (const row of candidates.values()) {
      const embedding = decodeEmbedding(row.embeddingJson) ?? []
      const semanticScore = Math.max(0, cosineSimilarity(queryEmbedding, embedding))
      const lexicalIndex = lexicalRows.findIndex((candidate) => candidate.id === row.id)
      const lexicalScore = lexicalIds.has(row.id)
        ? Math.max(0.15, 1 - lexicalIndex / Math.max(lexicalRows.length, 1))
        : 0
      const score = lexicalScore * 0.72 + semanticScore * 0.28
      if (score <= 0.08) continue
      byId.set(row.id, {
        chunkId: row.id,
        sourceId: row.sourceId,
        path: row.path,
        title: basename(row.path),
        pageNumber: row.pageNumber ?? undefined,
        text: row.text,
        score,
        lexicalScore,
        semanticScore,
      })
    }

    return Array.from(byId.values())
      .sort((left, right) => right.score - left.score)
      .slice(0, limit)
  }

  readChunk(chunkId: string, maxChars = 12_000, rootIds?: string[]): KnowledgeReadResult | null {
    this.ensureInitialized()
    if (!chunkId || rootIds?.length === 0) return null
    const rootFilter = rootIds ? `AND r.id IN (${rootIds.map(() => '?').join(', ')})` : ''
    const selected = this.db
      .prepare(
        `
      SELECT c.rowid AS rowId, c.source_id AS sourceId, c.page_number AS pageNumber,
             s.path
      FROM knowledge_chunks c
      JOIN knowledge_sources s ON s.id = c.source_id
      JOIN knowledge_roots r ON r.id = s.root_id
      WHERE c.id = ? AND r.enabled = 1 ${rootFilter}
      LIMIT 1
    `
      )
      .get(chunkId, ...(rootIds ?? [])) as
      | {
          rowId: number
          sourceId: string
          pageNumber: number | null
          path: string
        }
      | undefined
    if (!selected) return null

    const nearby = this.db
      .prepare(
        `
      SELECT text FROM knowledge_chunks
      WHERE source_id = ? AND rowid BETWEEN ? AND ?
      ORDER BY rowid ASC
    `
      )
      .all(selected.sourceId, selected.rowId - 2, selected.rowId + 2) as Array<{ text: string }>
    const text = nearby
      .map((row) => row.text)
      .join('\n\n')
      .slice(0, Math.max(500, maxChars))
    this.db
      .prepare(
        `
      UPDATE knowledge_pages SET last_accessed_at = ?
      WHERE source_id = ? AND page_number = ?
    `
      )
      .run(Date.now(), selected.sourceId, selected.pageNumber ?? 1)
    return {
      resultId: chunkId,
      sourceId: selected.sourceId,
      path: selected.path,
      title: basename(selected.path),
      pageNumber: selected.pageNumber ?? undefined,
      text,
    }
  }
}
