import Database from 'better-sqlite3'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { SearchIndexDatabase } from './indexDb'

describe('search index hot usage window', () => {
  let appDataDir = ''
  let previousAppDataDir: string | undefined

  beforeEach(() => {
    previousAppDataDir = process.env.APPDATA_DIR
    appDataDir = mkdtempSync(join(tmpdir(), 'raymes-search-index-'))
    process.env.APPDATA_DIR = appDataDir
  })

  afterEach(() => {
    if (previousAppDataDir === undefined) delete process.env.APPDATA_DIR
    else process.env.APPDATA_DIR = previousAppDataDir
    rmSync(appDataDir, { recursive: true, force: true })
  })

  it('persists each successful action and exposes its five-minute use count', async () => {
    const index = new SearchIndexDatabase()
    await index.ensureInitialized()
    index.upsertDocuments([
      {
        id: 'native:hot-command',
        category: 'native-command',
        title: 'Hot Command',
        subtitle: 'Fixture',
        tokens: 'hot command fixture',
        action: { type: 'run-native-command', commandId: 'hot-command' },
        updatedAt: Date.now(),
      },
    ])

    index.recordAction('native:hot-command', true)
    index.recordAction('native:hot-command', true)
    index.recordAction('native:hot-command', true)
    index.recordAction('native:failed-command', false)

    expect(index.getActionStats(['native:hot-command']).get('native:hot-command')).toMatchObject({
      frequency: 3,
      recentUseCount: 3,
    })
    expect(index.listRecommendedDocuments(10)[0]).toMatchObject({
      id: 'native:hot-command',
      recentUseCount: 3,
    })
  })

  it('migrates the legacy unindexed-id FTS table and keeps updates synchronized by rowid', async () => {
    const searchDir = join(appDataDir, 'search')
    const databasePath = join(searchDir, 'index.sqlite3')
    mkdirSync(searchDir, { recursive: true })
    const legacy = new Database(databasePath)
    legacy.exec(`
      CREATE TABLE documents (
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
      CREATE VIRTUAL TABLE documents_fts USING fts5(
        id UNINDEXED,
        title,
        subtitle,
        tokens,
        tokenize = 'unicode61'
      );
      INSERT INTO documents VALUES (
        'legacy:one', 'commands', 'Legacy Command', 'Fixture', 'legacy command',
        '{"type":"copy-text","text":"legacy"}', 1, NULL, NULL, 0
      );
      INSERT INTO documents_fts(id, title, subtitle, tokens)
      VALUES ('legacy:one', 'Legacy Command', 'Fixture', 'legacy command');
    `)
    ;(legacy as unknown as { close?: () => void }).close?.()

    const index = new SearchIndexDatabase()
    await index.ensureInitialized()
    expect(index.search('legacy', 10).map((row) => row.id)).toContain('legacy:one')

    index.upsertDocuments([
      {
        id: 'legacy:one',
        category: 'commands',
        title: 'Modern Command',
        subtitle: 'Fixture',
        tokens: 'modern command',
        action: { type: 'copy-text', text: 'modern' },
        updatedAt: 2,
      },
    ])
    expect(index.search('legacy', 10).map((row) => row.id)).not.toContain('legacy:one')
    expect(index.search('modern', 10).map((row) => row.id)).toContain('legacy:one')

    index.removeDocumentById('legacy:one')
    expect(index.search('modern', 10)).toEqual([])

    const migrated = new Database(databasePath, { readonly: true })
    const schema = migrated
      .prepare("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'documents_fts'")
      .get() as { sql: string }
    expect(schema.sql).toContain("content = 'documents'")
    ;(migrated as unknown as { close?: () => void }).close?.()
  })
})
