import { app } from 'electron'
import DatabaseCtor, { type Database as DatabaseType } from 'better-sqlite3'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import type { SearchAction, SearchCategory } from '../../shared/search'
import { buildFtsQuery, levenshteinDistance, lexicalScore } from './textMatch'
import type { IndexedDocument } from './providers/types'

export type SearchIndexRow = {
  id: string
  category: SearchCategory
  title: string
  subtitle: string
  actionJson: string
  updatedAt: number
  lexical: number
  fuzzyDistance?: number
  popularity: number
}

type RecommendedIndexRow = {
  id: string
  category: SearchCategory
  title: string
  subtitle: string
  actionJson: string
  updatedAt: number
  frequency: number
  successCount: number
  totalCount: number
  lastUsedAt: number
}

type ActionStats = {
  actionId: string
  frequency: number
  successCount: number
  totalCount: number
  lastUsedAt: number
}

function dbPath(): string {
  const dir = join(app.getPath('userData'), 'search')
  mkdirSync(dir, { recursive: true })
  return join(dir, 'index.sqlite3')
}

function safeJsonParse<T>(value: string, fallback: T): T {
  try {
    return JSON.parse(value) as T
  } catch {
    return fallback
  }
}

const CLICK_EVENTS_RETAIN = 1000
const BENCHMARK_SNAPSHOTS_RETAIN = 50

export async function readBenchmarkHistory() {
  return [];
}

export async function runOfflineBenchmarks(
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  _searchFn: (q: string) => Promise<unknown[]>,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  _db: unknown,
) {
  // Benchmark implementation omitted for brevity
}

// Module-level singleton with lazy initialization
let _instance: SearchIndexDatabase | null = null

export function getInstance(): SearchIndexDatabase {
  if (!_instance) {
    _instance = new SearchIndexDatabase()
  }
  return _instance
}

export class SearchIndexDatabase {
  private _db: DatabaseType | null = null
  private _initPromise: Promise<void> | null = null

  private get db(): DatabaseType {
    if (!this._db) {
      throw new Error('Database not initialized - call ensureInitialized() first')
    }
    return this._db
  }

  async ensureInitialized(): Promise<void> {
    if (this._initPromise) return this._initPromise

    this._initPromise = new Promise((resolve) => {
      // Defer database initialization to avoid blocking app startup
      setImmediate(() => {
        this._db = new DatabaseCtor(dbPath())
        this._db.pragma('journal_mode = WAL')
        this._db.pragma('synchronous = NORMAL')
        this.bootstrap()
        resolve()
      })
    })

    return this._initPromise
  }

  private bootstrap(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS documents (
        id TEXT PRIMARY KEY,
        category TEXT NOT NULL,
        title TEXT NOT NULL,
        subtitle TEXT NOT NULL,
        tokens TEXT NOT NULL,
        action_json TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        source_path TEXT,
        source_mtime INTEGER,
        popularity REAL NOT NULL DEFAULT 0
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
        id UNINDEXED,
        title,
        subtitle,
        tokens,
        tokenize = 'unicode61'
      );

      CREATE TABLE IF NOT EXISTS action_stats (
        action_id TEXT PRIMARY KEY,
        frequency INTEGER NOT NULL DEFAULT 0,
        success_count INTEGER NOT NULL DEFAULT 0,
        total_count INTEGER NOT NULL DEFAULT 0,
        last_used_at INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS benchmark_snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        created_at INTEGER NOT NULL,
        precision_at_5 REAL NOT NULL,
        precision_at_10 REAL NOT NULL,
        avg_click_rank REAL NOT NULL,
        benchmark_size INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS click_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        created_at INTEGER NOT NULL,
        query TEXT NOT NULL,
        result_id TEXT NOT NULL,
        rank INTEGER NOT NULL,
        success INTEGER NOT NULL
      );
    `)

    this.ensureDocumentsSchema()
    this.pruneTelemetry()
  }

  /** Remove old click-events and benchmark snapshots so the DB doesn't grow
   *  without bound. Retention limits are conservative — enough for ranking
   *  learning and debugging without unbounded disk use. */
  private pruneTelemetry(): void {
    try {
      this.db
        .prepare(
          `DELETE FROM click_events WHERE id NOT IN (
            SELECT id FROM click_events ORDER BY id DESC LIMIT ?
          )`,
        )
        .run(CLICK_EVENTS_RETAIN)
      this.db
        .prepare(
          `DELETE FROM benchmark_snapshots WHERE id NOT IN (
            SELECT id FROM benchmark_snapshots ORDER BY id DESC LIMIT ?
          )`,
        )
        .run(BENCHMARK_SNAPSHOTS_RETAIN)
    } catch (error) {
      console.warn('[SearchIndex] Telemetry pruning failed:', error)
    }
  }

  /** Run WAL checkpoint and VACUUM to reclaim disk space. */
  vacuum(): void {
    this.db.exec('PRAGMA wal_checkpoint(TRUNCATE); VACUUM;')
  }

  /** Forward-compatible schema patching for users with older local DBs. */
  private ensureDocumentsSchema(): void {
    const rows = this.db.prepare('PRAGMA table_info(documents)').all() as Array<{ name: string }>
    const columns = new Set(rows.map((row) => row.name))

    if (!columns.has('source_path')) {
      this.db.exec('ALTER TABLE documents ADD COLUMN source_path TEXT')
    }
    if (!columns.has('source_mtime')) {
      this.db.exec('ALTER TABLE documents ADD COLUMN source_mtime INTEGER')
    }
    if (!columns.has('popularity')) {
      this.db.exec('ALTER TABLE documents ADD COLUMN popularity REAL NOT NULL DEFAULT 0')
    }
  }

  upsertDocuments(documents: IndexedDocument[]): void {
    if (documents.length === 0) return

    const upsertDoc = this.db.prepare(`
      INSERT INTO documents (id, category, title, subtitle, tokens, action_json, updated_at, source_path, source_mtime, popularity)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        category = excluded.category,
        title = excluded.title,
        subtitle = excluded.subtitle,
        tokens = excluded.tokens,
        action_json = excluded.action_json,
        updated_at = excluded.updated_at,
        source_path = excluded.source_path,
        source_mtime = excluded.source_mtime,
        popularity = excluded.popularity
    `)

    const deleteFts = this.db.prepare('DELETE FROM documents_fts WHERE id = ?')
    const insertFts = this.db.prepare(
      'INSERT INTO documents_fts (id, title, subtitle, tokens) VALUES (?, ?, ?, ?)',
    )

    const upsertTx = this.db.transaction((rows: IndexedDocument[]) => {
      for (const row of rows) {
        upsertDoc.run(
          row.id,
          row.category,
          row.title,
          row.subtitle,
          row.tokens,
          JSON.stringify(row.action),
          Math.round(row.updatedAt || Date.now()),
          row.sourcePath ?? null,
          row.sourceMtime ? Math.round(row.sourceMtime) : null,
          row.popularity ?? 0,
        )
        deleteFts.run(row.id)
        insertFts.run(row.id, row.title, row.subtitle, row.tokens)
      }
    })

    upsertTx(documents)
  }

  removeDocumentById(id: string): void {
    this.db.prepare('DELETE FROM documents WHERE id = ?').run(id)
    this.db.prepare('DELETE FROM documents_fts WHERE id = ?').run(id)
  }

  removeDocumentsByCategory(category: SearchCategory): number {
    const ids = this.db
      .prepare('SELECT id FROM documents WHERE category = ?')
      .all(category) as { id: string }[]
    if (ids.length === 0) return 0
    const delDoc = this.db.prepare('DELETE FROM documents WHERE id = ?')
    const delFts = this.db.prepare('DELETE FROM documents_fts WHERE id = ?')
    const removeTx = this.db.transaction((rows: { id: string }[]) => {
      for (const row of rows) {
        delDoc.run(row.id)
        delFts.run(row.id)
      }
    })
    removeTx(ids)
    return ids.length
  }

  replaceDocumentsByCategory(
    category: SearchCategory,
    documents: IndexedDocument[],
  ): void {
    const deleteFts = this.db.prepare(
      'DELETE FROM documents_fts WHERE id IN (SELECT id FROM documents WHERE category = ?)',
    )
    const deleteDocuments = this.db.prepare('DELETE FROM documents WHERE category = ?')
    const replaceTx = this.db.transaction(() => {
      deleteFts.run(category)
      deleteDocuments.run(category)
      this.upsertDocuments(documents)
    })
    replaceTx()
    this.clearSearchCache()
  }

  search(query: string, limit: number): SearchIndexRow[] {
    const ftsQuery = buildFtsQuery(query)
    const trimmed = query.trim()
    if (!trimmed) return []
    const candidateLimit = Math.max(limit * 2, 20)

    const rows =
      ftsQuery.length > 0
        ? this.db
            .prepare(
              `
                SELECT d.id AS id,
                       d.category AS category,
                       d.title AS title,
                       d.subtitle AS subtitle,
                       d.action_json AS actionJson,
                       d.updated_at AS updatedAt,
                       d.popularity AS popularity,
                       bm25(documents_fts, 5.0, 2.0, 1.0) AS bm25Score
                FROM documents_fts
                JOIN documents d ON d.id = documents_fts.id
                WHERE documents_fts MATCH ?
                ORDER BY bm25Score ASC
                LIMIT ?
              `,
            )
            .all(ftsQuery, candidateLimit)
        : []

    const mapped = (rows as Array<{ id: string; category: SearchCategory; title: string; subtitle: string; actionJson: string; updatedAt: number; popularity: number; bm25Score: number }>).map((row) => {
      const inverseBm25 = Number.isFinite(row.bm25Score) ? 1 / (1 + Math.max(row.bm25Score, 0)) : 0.5
      const lexical = Math.max(inverseBm25, lexicalScore(`${row.title} ${row.subtitle}`, trimmed))
      return {
        id: row.id,
        category: row.category,
        title: row.title,
        subtitle: row.subtitle,
        actionJson: row.actionJson,
        updatedAt: row.updatedAt,
        lexical,
        popularity: row.popularity,
      } satisfies SearchIndexRow
    })

    if (mapped.length >= candidateLimit) {
      return mapped.slice(0, candidateLimit)
    }

    return [...mapped, ...this.fuzzySearch(trimmed, candidateLimit - mapped.length)]
  }

  private fuzzySearch(query: string, limit: number): SearchIndexRow[] {
    if (limit <= 0) return []

    const rows = this.db
      .prepare(
        `
          SELECT id, category, title, subtitle, action_json AS actionJson, updated_at AS updatedAt, popularity
          FROM documents
          ORDER BY updated_at DESC
          LIMIT ?
        `,
      )
      .all(Math.max(300, limit * 30)) as Array<{
      id: string
      category: SearchCategory
      title: string
      subtitle: string
      actionJson: string
      updatedAt: number
      popularity: number
    }>

    const scored: SearchIndexRow[] = []
    for (const row of rows) {
      const candidate = row.title.toLowerCase()
      const distance = levenshteinDistance(candidate, query.toLowerCase())
      if (distance > 3 && !candidate.includes(query.toLowerCase())) continue
      const lexical = lexicalScore(`${row.title} ${row.subtitle}`, query)
      scored.push({
        id: row.id,
        category: row.category,
        title: row.title,
        subtitle: row.subtitle,
        actionJson: row.actionJson,
        updatedAt: row.updatedAt,
        lexical,
        fuzzyDistance: distance,
        popularity: row.popularity,
      })
    }

    scored.sort((a, b) => {
      if (a.fuzzyDistance !== undefined && b.fuzzyDistance !== undefined && a.fuzzyDistance !== b.fuzzyDistance) {
        return a.fuzzyDistance - b.fuzzyDistance
      }
      return b.lexical - a.lexical
    })

    return scored.slice(0, limit)
  }

  parseAction(actionJson: string): SearchAction {
    return safeJsonParse<SearchAction>(actionJson, { type: 'copy-text', text: '' })
  }

  getActionStats(actionIds: string[]): Map<string, ActionStats> {
    if (actionIds.length === 0) return new Map()

    const placeholders = actionIds.map(() => '?').join(',')
    const rows = this.db
      .prepare(
        `
          SELECT action_id AS actionId,
                 frequency AS frequency,
                 success_count AS successCount,
                 total_count AS totalCount,
                 last_used_at AS lastUsedAt
          FROM action_stats
          WHERE action_id IN (${placeholders})
        `,
      )
      .all(...actionIds) as ActionStats[]

    return new Map(rows.map((row) => [row.actionId, row]))
  }

  getDocumentsByIds(ids: string[]): SearchIndexRow[] {
    if (ids.length === 0) return []
    const placeholders = ids.map(() => '?').join(',')
    const rows = this.db
      .prepare(
        `
          SELECT id,
                 category,
                 title,
                 subtitle,
                 action_json AS actionJson,
                 updated_at AS updatedAt,
                 popularity
          FROM documents
          WHERE id IN (${placeholders})
        `,
      )
      .all(...ids) as Array<{
      id: string
      category: SearchCategory
      title: string
      subtitle: string
      actionJson: string
      updatedAt: number
      popularity: number
    }>

    return rows.map((row) => ({
      id: row.id,
      category: row.category,
      title: row.title,
      subtitle: row.subtitle,
      actionJson: row.actionJson,
      updatedAt: row.updatedAt,
      lexical: 0,
      popularity: row.popularity,
    }))
  }

  listRecommendedDocuments(limit: number): RecommendedIndexRow[] {
    if (limit <= 0) return []

    return this.db
      .prepare(
        `
          SELECT d.id AS id,
                 d.category AS category,
                 d.title AS title,
                 d.subtitle AS subtitle,
                 d.action_json AS actionJson,
                 d.updated_at AS updatedAt,
                 COALESCE(a.frequency, 0) AS frequency,
                 COALESCE(a.success_count, 0) AS successCount,
                 COALESCE(a.total_count, 0) AS totalCount,
                 COALESCE(a.last_used_at, 0) AS lastUsedAt
          FROM documents d
          LEFT JOIN action_stats a ON a.action_id = d.id
          WHERE d.category <> 'files'
          ORDER BY
            CASE WHEN COALESCE(a.last_used_at, 0) > 0 THEN 0 ELSE 1 END ASC,
            COALESCE(a.last_used_at, 0) DESC,
            COALESCE(a.frequency, 0) DESC,
            d.updated_at DESC
          LIMIT ?
        `,
      )
      .all(limit) as RecommendedIndexRow[]
  }

  recordAction(actionId: string, success: boolean): void {
    const now = Date.now()
    this.db
      .prepare(
        `
          INSERT INTO action_stats (action_id, frequency, success_count, total_count, last_used_at)
          VALUES (?, 1, ?, 1, ?)
          ON CONFLICT(action_id) DO UPDATE SET
            frequency = action_stats.frequency + 1,
            success_count = action_stats.success_count + excluded.success_count,
            total_count = action_stats.total_count + 1,
            last_used_at = excluded.last_used_at
        `,
      )
      .run(actionId, success ? 1 : 0, now)
  }

  recordClick(query: string, resultId: string, rank: number, success: boolean): void {
    this.db
      .prepare(
        'INSERT INTO click_events (created_at, query, result_id, rank, success) VALUES (?, ?, ?, ?, ?)',
      )
      .run(Date.now(), query, resultId, rank, success ? 1 : 0)
  }

  readRecentClickAverage(limit = 200): number {
    const rows = this.db
      .prepare('SELECT rank FROM click_events ORDER BY id DESC LIMIT ?')
      .all(limit) as Array<{ rank: number }>
    if (rows.length === 0) return 0
    const sum = rows.reduce((acc, row) => acc + row.rank, 0)
    return sum / rows.length
  }

  writeBenchmarkSnapshot(precisionAt5: number, precisionAt10: number, benchmarkSize: number): void {
    this.db
      .prepare(
        'INSERT INTO benchmark_snapshots (created_at, precision_at_5, precision_at_10, avg_click_rank, benchmark_size) VALUES (?, ?, ?, ?, ?)',
      )
      .run(Date.now(), precisionAt5, precisionAt10, this.readRecentClickAverage(), benchmarkSize)
  }

  readBenchmarkHistory(limit = 40): Array<{ createdAt: number; precisionAt5: number; precisionAt10: number; avgClickRank: number }> {
    return this.db
      .prepare(
        `SELECT created_at AS createdAt,
                precision_at_5 AS precisionAt5,
                precision_at_10 AS precisionAt10,
                avg_click_rank AS avgClickRank
        FROM benchmark_snapshots
        ORDER BY id DESC
        LIMIT ?
      `,
      )
      .all(limit) as Array<{ createdAt: number; precisionAt5: number; precisionAt10: number; avgClickRank: number }>
  }

  // Session cache for search results
  private _searchCache: Map<string, SearchIndexRow[]> = new Map()
  private _cacheTimestamp: Map<string, number> = new Map()
  private readonly CACHE_TTL = 5 * 60 * 1000 // 5 minutes

  getSearch(query: string, limit: number): SearchIndexRow[] {
    const now = Date.now()
    const cacheKey = `${query}:${limit}`

    // Check cache validity
    const lastUpdate = this._cacheTimestamp.get(cacheKey)
    if (lastUpdate && now - lastUpdate < this.CACHE_TTL) {
      return this._searchCache.get(cacheKey) || []
    }

    // Perform search and cache result
    const results = this.search(query, limit)
    this._searchCache.set(cacheKey, results)
    this._cacheTimestamp.set(cacheKey, now)

    return results
  }

  clearSearchCache(): void {
    this._searchCache.clear()
    this._cacheTimestamp.clear()
  }
}
