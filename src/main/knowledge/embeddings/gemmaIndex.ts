import type { Database } from 'better-sqlite3'
import type { KnowledgeSearchHit } from '../../../shared/knowledge'
import { basename } from 'node:path'
import { GEMMA_INDEX_KEY, GEMMA_MODEL } from './gemmaManifest'
import { cosineSimilarity } from './featureEmbedding'

export type GemmaChunk = { id: string; text: string; title: string }

function encode(vector: number[]): Buffer {
  if (vector.length !== GEMMA_MODEL.dimensions || vector.some((v) => !Number.isFinite(v))) {
    throw new Error('Invalid vector for the Gemma index')
  }
  const buffer = Buffer.alloc(vector.length * 4)
  vector.forEach((v, index) => buffer.writeFloatLE(v, index * 4))
  return buffer
}

function decode(buffer: Uint8Array): number[] {
  const bytes = Buffer.from(buffer)
  return Array.from({ length: bytes.length / 4 }, (_, index) => bytes.readFloatLE(index * 4))
}

/** Owns the learned vector space, independently of the built-in hashing index. */
export class GemmaIndex {
  constructor(
    private readonly db: Database,
    private readonly vectorAvailable: boolean
  ) {
    db.exec(`
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
    `)
    if (vectorAvailable) {
      db.exec(`
        CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_gemma_vectors USING vec0(
          embedding float[256] distance_metric=cosine,
          root_id text
        );
        CREATE TRIGGER IF NOT EXISTS knowledge_gemma_vector_delete
        BEFORE DELETE ON knowledge_chunks BEGIN
          DELETE FROM knowledge_gemma_vectors WHERE rowid = OLD.rowid;
        END;
      `)
    }
  }

  counts(rootIds: string[]): { indexedChunks: number; totalChunks: number } {
    if (!rootIds.length) return { indexedChunks: 0, totalChunks: 0 }
    return this.db
      .prepare(
        `
      SELECT COUNT(*) AS totalChunks,
             COUNT(g.chunk_id) AS indexedChunks
      FROM knowledge_chunks c INDEXED BY knowledge_gemma_ready_chunks_idx
      JOIN knowledge_sources s ON s.id = c.source_id
      LEFT JOIN knowledge_gemma_embeddings g ON g.chunk_id = c.id AND g.model_key = ?
      WHERE s.status = 'indexed' AND c.embedding_json IS NOT NULL
        AND s.root_id IN (${rootIds.map(() => '?').join(',')})
    `
      )
      .get(GEMMA_INDEX_KEY, ...rootIds) as { indexedChunks: number; totalChunks: number }
  }

  hasEmbeddings(rootIds: string[]): boolean {
    if (!rootIds.length) return false
    return Boolean(
      this.db
        .prepare(
          `
      SELECT 1 FROM knowledge_gemma_embeddings g
      JOIN knowledge_chunks c ON c.id = g.chunk_id
      JOIN knowledge_sources s ON s.id = c.source_id
      WHERE g.model_key = ? AND s.status = 'indexed'
        AND s.root_id IN (${rootIds.map(() => '?').join(',')}) LIMIT 1
    `
        )
        .get(GEMMA_INDEX_KEY, ...rootIds)
    )
  }

  pending(rootIds: string[], limit = 4, afterRowId = 0): Array<GemmaChunk & { rowId: number }> {
    if (!rootIds.length) return []
    // Keep chunks first in rowid order, without sorting the corpus and its text for a small LIMIT.
    const rows = this.db
      .prepare(
        `
      SELECT c.rowid AS rowId, c.id, c.text, s.path
      FROM knowledge_chunks c CROSS JOIN knowledge_sources s
      LEFT JOIN knowledge_gemma_embeddings g ON g.chunk_id = c.id
      WHERE s.id = c.source_id AND c.rowid > ?
        AND s.status = 'indexed' AND c.embedding_json IS NOT NULL
        AND s.root_id IN (${rootIds.map(() => '?').join(',')})
        AND (g.chunk_id IS NULL OR g.model_key != ?)
      ORDER BY c.rowid LIMIT ?
    `
      )
      .all(afterRowId, ...rootIds, GEMMA_INDEX_KEY, limit) as Array<{
      rowId: number
      id: string
      text: string
      path: string
    }>
    return rows.map((row) => ({
      rowId: row.rowId,
      id: row.id,
      text: row.text,
      title: basename(row.path),
    }))
  }

  save(chunks: GemmaChunk[], vectors: number[][]): number {
    if (chunks.length !== vectors.length) throw new Error('Embedding batch size does not match')
    const encoded = vectors.map(encode)
    const find = this.db.prepare(`
      SELECT c.rowid AS rowId, s.root_id AS rootId FROM knowledge_chunks c
      JOIN knowledge_sources s ON s.id = c.source_id
      WHERE c.id = ? AND c.text = ? AND s.status = 'indexed' AND c.embedding_json IS NOT NULL
    `)
    const insert = this.db.prepare(
      'INSERT OR REPLACE INTO knowledge_gemma_embeddings(chunk_id, model_key, vector) VALUES (?, ?, ?)'
    )
    const deleteVector = this.vectorAvailable
      ? this.db.prepare('DELETE FROM knowledge_gemma_vectors WHERE rowid = ?')
      : null
    const insertVector = this.vectorAvailable
      ? this.db.prepare(
          'INSERT INTO knowledge_gemma_vectors(rowid, embedding, root_id) VALUES (?, ?, ?)'
        )
      : null
    let saved = 0
    this.db.transaction(() => {
      chunks.forEach((chunk, index) => {
        // Extraction/removal may have changed the chunk while inference ran.
        const current = find.get(chunk.id, chunk.text) as
          | { rowId: number; rootId: string }
          | undefined
        if (!current) return
        insert.run(chunk.id, GEMMA_INDEX_KEY, encoded[index])
        deleteVector?.run(BigInt(current.rowId))
        insertVector?.run(BigInt(current.rowId), encoded[index], current.rootId)
        saved++
      })
    })()
    return saved
  }

  search(vector: number[], rootIds: string[], limit: number): KnowledgeSearchHit[] {
    if (!rootIds.length || limit <= 0) return []
    const encoded = encode(vector)
    const rootFilter = rootIds.map(() => '?').join(',')
    const rows = this.vectorAvailable
      ? this.db
          .prepare(
            `
      WITH nearest AS (
        SELECT rowid, distance FROM knowledge_gemma_vectors
        WHERE embedding MATCH ? AND k = ? AND root_id IN (${rootFilter})
      )
      SELECT c.id, c.source_id AS sourceId, c.text, c.page_number AS pageNumber,
             s.path, g.vector, nearest.distance
      FROM nearest JOIN knowledge_chunks c ON c.rowid = nearest.rowid
      JOIN knowledge_sources s ON s.id = c.source_id
      JOIN knowledge_gemma_embeddings g ON g.chunk_id = c.id
      WHERE s.status = 'indexed' AND g.model_key = ? ORDER BY nearest.distance
    `
          )
          .all(encoded, Math.max(40, limit * 4), ...rootIds, GEMMA_INDEX_KEY)
      : this.db
          .prepare(
            `
      SELECT c.id, c.source_id AS sourceId, c.text, c.page_number AS pageNumber,
             s.path, g.vector
      FROM knowledge_gemma_embeddings g JOIN knowledge_chunks c ON c.id = g.chunk_id
      JOIN knowledge_sources s ON s.id = c.source_id
      WHERE g.model_key = ? AND s.status = 'indexed' AND s.root_id IN (${rootFilter})
    `
          )
          .all(GEMMA_INDEX_KEY, ...rootIds)
    return (
      rows as Array<{
        id: string
        sourceId: string
        text: string
        pageNumber: number | null
        path: string
        vector: Uint8Array
      }>
    )
      .map((row) => {
        const similarity = Math.max(0, cosineSimilarity(vector, decode(row.vector)))
        return {
          chunkId: row.id,
          sourceId: row.sourceId,
          path: row.path,
          title: basename(row.path),
          pageNumber: row.pageNumber ?? undefined,
          text: row.text,
          score: similarity,
          semanticScore: similarity,
          lexicalScore: 0,
        }
      })
      .sort((left, right) => right.score - left.score)
      .slice(0, limit)
  }

  clear(): void {
    this.db.transaction(() => {
      this.db.prepare('DELETE FROM knowledge_gemma_embeddings').run()
      if (this.vectorAvailable) this.db.prepare('DELETE FROM knowledge_gemma_vectors').run()
    })()
  }
}

/** Reciprocal rank fusion avoids treating BM25 ranks and cosine as the same scale. */
export function fuseKnowledgeHits(
  lexical: KnowledgeSearchHit[],
  semantic: KnowledgeSearchHit[],
  limit: number
): KnowledgeSearchHit[] {
  const scores = new Map<string, KnowledgeSearchHit>()
  for (const [kind, hits] of [
    ['lexical', lexical],
    ['semantic', semantic],
  ] as const) {
    hits.forEach((hit, rank) => {
      const previous = scores.get(hit.chunkId)
      scores.set(hit.chunkId, {
        ...hit,
        lexicalScore: kind === 'lexical' ? hit.lexicalScore : (previous?.lexicalScore ?? 0),
        semanticScore: kind === 'semantic' ? hit.semanticScore : (previous?.semanticScore ?? 0),
        score: (previous?.score ?? 0) + 30 / (60 + rank + 1),
      })
    })
  }
  return [...scores.values()].sort((left, right) => right.score - left.score).slice(0, limit)
}
