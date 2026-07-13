import { randomUUID } from 'node:crypto'
import { chmod, mkdir, mkdtemp, rm, truncate, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import {
  discoverMajorKnowledgeFolders,
  isKnowledgeCandidatePath,
  KnowledgeService,
  shouldSkipKnowledgeEntry,
} from './service'
import { getKnowledgeStore } from './database/store'
import { isIndexablePath } from './extractors/localExtractor'

const cleanup: string[] = []

afterEach(async () => {
  await Promise.all(cleanup.splice(0).map((path) => rm(path, { recursive: true, force: true })))
})

async function waitUntilComplete(service: KnowledgeService): Promise<void> {
  const deadline = Date.now() + 5_000
  while (Date.now() < deadline) {
    const state = service.snapshot().status.state
    if (state === 'completed') return
    if (state === 'failed') throw new Error(service.snapshot().status.error || 'Indexing failed')
    await new Promise((resolve) => setTimeout(resolve, 25))
  }
  throw new Error('Timed out waiting for knowledge indexing')
}

describe('knowledge service', () => {
  it('discovers useful major folders and rejects system/generated entries', async () => {
    const home = await mkdtemp(join(tmpdir(), 'tezbar-major-folders-'))
    cleanup.push(home)
    await Promise.all(
      ['Desktop', 'Documents', 'Downloads', 'Pictures', 'Library'].map((name) =>
        mkdir(join(home, name))
      )
    )

    expect(discoverMajorKnowledgeFolders(home)).toEqual([
      join(home, 'Desktop'),
      join(home, 'Documents'),
      join(home, 'Downloads'),
      join(home, 'Pictures'),
    ])
    expect(shouldSkipKnowledgeEntry('Library', true)).toBe(true)
    expect(shouldSkipKnowledgeEntry('node_modules', true)).toBe(true)
    expect(shouldSkipKnowledgeEntry('Photos Library.photoslibrary', true)).toBe(true)
    expect(shouldSkipKnowledgeEntry('Example.app', true)).toBe(true)
    expect(shouldSkipKnowledgeEntry('package-lock.json', false)).toBe(true)
    expect(shouldSkipKnowledgeEntry('bundle.min.js', false)).toBe(true)
    expect(shouldSkipKnowledgeEntry('service-account-credentials.json', false)).toBe(true)
    expect(shouldSkipKnowledgeEntry('private.pem', false)).toBe(true)
    expect(shouldSkipKnowledgeEntry('proposal.pdf', false)).toBe(false)
    expect(isIndexablePath('/tmp/extensionless-cache-entry')).toBe(false)
    expect(
      isKnowledgeCandidatePath(
        '/tmp/Pictures',
        '/tmp/Pictures/Photos Library.photoslibrary/resources/thumbnail.jpeg',
      ),
    ).toBe(false)
    expect(isKnowledgeCandidatePath('/tmp/Documents', '/tmp/Documents/proposal.pdf')).toBe(true)
  })

  it('indexes an approved folder and exposes the same search results to all consumers', async () => {
    const root = await mkdtemp(join(tmpdir(), 'tezbar-knowledge-root-'))
    const data = await mkdtemp(join(tmpdir(), 'tezbar-knowledge-data-'))
    cleanup.push(root, data)
    process.env.APPDATA_DIR = join(data, randomUUID())
    await writeFile(
      join(root, 'renewal.txt'),
      'The Almaty office certificate renewal deadline is September 18.',
      'utf8'
    )
    await mkdir(join(root, 'Library'))
    await mkdir(join(root, 'node_modules'))
    await writeFile(join(root, 'Library', 'system-notes.txt'), 'do not index system data', 'utf8')
    await writeFile(
      join(root, 'node_modules', 'dependency.txt'),
      'do not index dependencies',
      'utf8'
    )
    await writeFile(join(root, 'package-lock.json'), '{"generated":"lockfile"}', 'utf8')
    await writeFile(join(root, 'api-credentials.json'), '{"token":"do-not-index"}', 'utf8')
    const nativeHelper = join(root, 'native-helper')
    await writeFile(nativeHelper, Buffer.from([0x7f, 0x45, 0x4c, 0x46]))
    await chmod(nativeHelper, 0o755)
    const oversizedPath = join(root, 'oversized-generated.txt')
    await writeFile(oversizedPath, '', 'utf8')
    await truncate(oversizedPath, 65 * 1024 * 1024)

    const service = new KnowledgeService()
    let restartedService: KnowledgeService | null = null
    try {
      await service.setDepth('basic')
      service.addRoot(root)
      await waitUntilComplete(service)
      const hits = service.search('Almaty certificate renewal', 5)
      expect(hits[0]?.path).toBe(join(root, 'renewal.txt'))
      expect(hits[0]?.text).toContain('September 18')
      expect(hits[0]?.semanticScore).toBe(0)
      expect(service.snapshot().settings.depth).toBe('basic')
      expect(service.snapshot().roots[0]?.depth).toBe('inherit')
      expect(service.snapshot().status).toMatchObject({
        state: 'completed',
        sourceCount: 1,
        chunkCount: 1,
      })

      await service.pause()
      await writeFile(
        join(root, 'paused-change.txt'),
        'This file must wait until indexing is explicitly resumed.',
        'utf8'
      )
      await new Promise((resolve) => setTimeout(resolve, 1_750))
      expect(service.snapshot().status.state).toBe('paused')
      expect(service.snapshot().status.detail).toBe('Indexing paused')
      expect(service.search('explicitly resumed', 5)).toHaveLength(0)

      await service.resume()
      await waitUntilComplete(service)
      expect(service.search('explicitly resumed', 5)[0]?.path).toBe(join(root, 'paused-change.txt'))

      const rootId = service.snapshot().roots[0]?.id
      expect(rootId).toBeTruthy()
      await service.setRootDepth(rootId!, 'smart')
      await waitUntilComplete(service)
      expect(service.search('Almaty certificate renewal', 5)[0]?.semanticScore).toBeGreaterThan(0)
      expect(service.snapshot().roots[0]?.depth).toBe('smart')

      const interruptedPath = join(root, 'resume-after-restart.txt')
      await writeFile(
        interruptedPath,
        'The durable indexing queue resumes this file after an application restart.',
        'utf8'
      )
      getKnowledgeStore().saveIndexingCheckpoint('interrupted-job', [
        { rootId: rootId!, path: interruptedPath },
      ])
      service.shutdown()

      restartedService = new KnowledgeService()
      expect(restartedService.snapshot().status).toMatchObject({
        state: 'indexing',
        progress: 0,
        processedSources: 0,
        queuedSources: 1,
      })
      await restartedService.startIndexing()
      await waitUntilComplete(restartedService)
      expect(restartedService.snapshot().status).toMatchObject({
        state: 'completed',
        progress: 1,
        processedSources: 1,
        queuedSources: 0,
      })
      expect(restartedService.search('durable indexing queue', 5)[0]?.path).toBe(interruptedPath)
      expect(getKnowledgeStore().getIndexingCheckpoint()).toBeNull()
    } finally {
      service.shutdown()
      restartedService?.shutdown()
    }
  })
})
