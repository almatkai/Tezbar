import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { buildDeepSearchRecommendation } from '../../shared/searchMode'
import { optimisticSearchResults } from '../../renderer/optimisticSearch'
import { getInstance } from './indexDb'
import type { IndexedDocument } from './providers/types'

// Keep the real SQLite retrieval + service ranking. Only disable desktop I/O,
// background discovery, and icon generation; no search results are mocked.
vi.mock('./providers/appsProvider', () => ({
  appsProvider: { providerId: 'apps', buildDocuments: async () => [] },
  listApplications: () => [],
}))
vi.mock('./providers/commandsProvider', () => ({
  commandsProvider: { providerId: 'commands', buildDocuments: async () => [] },
}))
vi.mock('./providers/clipboardProvider', () => ({
  clipboardProvider: { providerId: 'clipboard', buildDocuments: async () => [] },
}))
vi.mock('./providers/notesProvider', () => ({
  notesProvider: { providerId: 'notes', buildDocuments: async () => [] },
}))
vi.mock('./providers/snippetsProvider', () => ({
  snippetsProvider: { providerId: 'snippets', buildDocuments: async () => [] },
}))
vi.mock('./providers/quickLinksProvider', () => ({
  quickLinksProvider: { providerId: 'quick-links', buildDocuments: async () => [] },
}))
vi.mock('./providers/filesProvider', () => ({
  scanInitialFileDocumentBatches: async function* () {},
  startFileWatcher: () => () => {},
}))
vi.mock('../knowledge/service', () => ({
  notifyKnowledgeInteractiveActivity: () => {},
  getKnowledgeService: () => {
    throw new Error('Basic searches must not invoke Deep Search')
  },
}))
vi.mock('../pathIcons', () => ({ fileIconDataUrl: () => undefined }))
vi.mock('../nativeCommands/executor', () => ({ executeNativeCommand: vi.fn() }))

const CASES = [
  ['Google Chrome', 'chrome', 'сркщьу', 'хром'],
  ['Telegram', 'telegram', 'еудупкфь', 'телеграм'],
  ['Discord', 'discord', 'вшысщкв', 'дискорд'],
  ['Docker', 'docker', 'вщслук', 'докер'],
  ['Safari', 'safari', 'ыфафкш', 'сафари'],
  ['Xcode', 'xcode', 'чсщву', 'икскод'],
  ['Visual Studio Code', 'vs code', 'мы сщву', 'vs сщву', 'мы code', 'вс код'],
]

describe('launcher search with a crowded persisted index', () => {
  let directory: string
  let previousAppDataDir: string | undefined
  let searchEverything: (typeof import('./service'))['searchEverything']
  const index = getInstance()
  const apps: IndexedDocument[] = CASES.map(([name]) => ({
    id: `app:${name}`,
    category: 'applications',
    title: name,
    subtitle: `/Applications/${name}.app`,
    // Persisted pre-fix rows have no alias tokens; they must work immediately.
    tokens: `${name} /Applications/${name}.app`,
    action: { type: 'open-app', appName: name },
    updatedAt: 1,
  }))

  beforeAll(async () => {
    previousAppDataDir = process.env.APPDATA_DIR
    directory = mkdtempSync(join(tmpdir(), 'tezbar-launcher-search-'))
    process.env.APPDATA_DIR = directory
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    const service = await import('./service')
    searchEverything = service.searchEverything
    await service.warmSearchIndex()
    await new Promise<void>((resolve) => setImmediate(resolve))
    index.upsertDocuments([
      ...apps,
      ...Array.from(
        { length: 2500 },
        (_, i): IndexedDocument => ({
          id: `file:${i}`,
          category: 'files',
          title: 'vs code',
          subtitle: `/projects/${i}/vs/code/settings.json`,
          tokens: 'vs code settings',
          action: { type: 'open-file', path: `/projects/${i}/vs/code/settings.json` },
          updatedAt: Date.now(),
        })
      ),
    ])
  })

  afterAll(() => {
    vi.clearAllTimers()
    vi.useRealTimers()
    index.close()
    if (previousAppDataDir === undefined) delete process.env.APPDATA_DIR
    else process.env.APPDATA_DIR = previousAppDataDir
    rmSync(directory, { recursive: true, force: true })
  })

  it.each(CASES.flatMap(([name, ...queries]) => queries.map((query) => [name, query])))(
    'selects %s first for %s, in the first frame and final response',
    async (name, query) => {
      const candidates = apps.map((doc) => ({ ...doc, score: 0 }))
      const firstFrame = optimisticSearchResults(candidates, query)
      expect(firstFrame[0]?.id).toBe(`app:${name}`)
      expect(buildDeepSearchRecommendation(query, firstFrame)).toBeNull()

      const results = await searchEverything(query)
      expect(results[0]?.id).toBe(`app:${name}`)
      expect(results[0]?.action).toEqual({ type: 'open-app', appName: name })
      expect(results.some((result) => result.id.startsWith('deep-search:'))).toBe(false)
    }
  )

  it('still offers Deep Search for genuinely unmatched input', async () => {
    const results = await searchEverything('несуществующее приложение')
    expect(results[0]?.id).toMatch(/^deep-search:/)
  })
})
