import { describe, expect, it } from 'vitest'
import { GemmaRuntime } from './gemmaRuntime'
import { cosineSimilarity } from './featureEmbedding'
import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { KnowledgeService } from '../service'
import { getKnowledgeStore } from '../database/store'

// Explicit opt-in: downloads the pinned model/runtime into APPDATA_DIR.
describe.skipIf(process.env.TEZBAR_GEMMA_INTEGRATION !== '1')('real local Gemma runtime', () => {
  it(
    'downloads, verifies, and retrieves paraphrases and another language offline',
    async () => {
      if (!process.env.APPDATA_DIR) throw new Error('Set APPDATA_DIR to an isolated test directory')
      const runtime = new GemmaRuntime()
      try {
        runtime.startDownload()
        const deadline = Date.now() + 15 * 60_000
        while (runtime.snapshot().modelState === 'downloading' && Date.now() < deadline) {
          await new Promise((resolve) => setTimeout(resolve, 500))
        }
        expect(runtime.snapshot().error).toBeUndefined()
        expect(runtime.snapshot().modelState).toBe('ready')
        const documents = await runtime.embedDocuments([
          {
            title: 'Physics',
            text: 'The northern lights are caused by charged particles from the sun interacting with Earth’s atmosphere.',
          },
          { title: 'Cooking', text: 'Mix flour, butter and sugar to bake a cake.' },
        ])
        for (const query of ['What causes the aurora borealis?', 'Что вызывает северное сияние?']) {
          const vector = await runtime.embedQuery(query)
          expect(vector).toHaveLength(256)
          expect(cosineSimilarity(vector, documents[0])).toBeGreaterThan(
            cosineSimilarity(vector, documents[1]) + 0.1
          )
        }
        runtime.stop()
        // Restart from installed files; no network is needed for inference.
        expect(await runtime.embedQuery('solar particles')).toHaveLength(256)
        // Multilingual chunks can tokenize much longer than Latin-script chunks.
        expect(
          (await runtime.embedDocuments([{ text: '太阳风与地球大气产生极光。'.repeat(90) }]))[0]
        ).toHaveLength(256)
      } finally {
        runtime.shutdown()
      }
    },
    20 * 60_000
  )

  it('backfills extracted content and uses it through the app search interface', async () => {
    if (!process.env.APPDATA_DIR) throw new Error('Set APPDATA_DIR to an isolated test directory')
    const root = join(process.env.APPDATA_DIR, 'test-documents')
    await mkdir(root, { recursive: true })
    await writeFile(
      join(root, 'physics.txt'),
      'The northern lights are caused by charged particles from the sun interacting with Earth’s atmosphere.'
    )
    await writeFile(join(root, 'cooking.txt'), 'Mix flour, butter and sugar to bake a cake.')
    const service = new KnowledgeService({ statusSink: () => {} })
    try {
      const existing = getKnowledgeStore()
        .listRoots()
        .find((candidate) => candidate.path === root)
      if (existing) await service.removeRoot(existing.id)
      service.addRoot(root)
      await service.waitForCurrentRun()
      const extracted = service.snapshot().status.lastCompletedAt
      await service.setEmbeddingModel('embeddinggemma-2')
      const deadline = Date.now() + 60_000
      while (service.snapshot().semanticSearch?.indexedChunks !== 2 && Date.now() < deadline) {
        const error = service.snapshot().semanticSearch?.error
        if (error) throw new Error(error)
        await new Promise((resolve) => setTimeout(resolve, 250))
      }
      expect(service.snapshot().semanticSearch?.indexedChunks).toBe(2)
      expect(service.snapshot().status.lastCompletedAt).toBe(extracted)
      const hits = await service.search('Что вызывает северное сияние?', 2)
      expect(hits[0].title).toBe('physics.txt')
      expect(hits[0].semanticScore).toBeGreaterThan(0)
      await service.setEmbeddingModel('built-in')
      expect((await service.search('flour butter', 2))[0].title).toBe('cooking.txt')
      await service.removeEmbeddingModel()
      expect(service.snapshot().semanticSearch?.modelState).toBe('not-downloaded')
      expect(
        getKnowledgeStore().getGemmaIndex().counts([service.snapshot().roots[0].id]).indexedChunks
      ).toBe(0)
      expect((await service.search('flour butter', 2))[0].title).toBe('cooking.txt')
    } finally {
      service.shutdown()
      getKnowledgeStore().close()
    }
  }, 90_000)
})
