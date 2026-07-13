import { mkdtempSync, rmSync } from 'node:fs'
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
})
