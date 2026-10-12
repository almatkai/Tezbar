import { randomUUID } from 'node:crypto'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { describe, expect, it, vi } from 'vitest'
import { KnowledgeStore } from '../database/store'
import { SemanticSearch } from './semanticSearch'
import type { GemmaRuntime } from './gemmaRuntime'
import { embedText, FEATURE_EMBEDDING_MODEL } from './featureEmbedding'

function fixture() {
  process.env.APPDATA_DIR = join(tmpdir(), `tezbar-semantic-${randomUUID()}`)
  const store = new KnowledgeStore()
  store.upsertRoot({
    id: 'root',
    path: '/test',
    depth: 'smart',
    processingBackend: 'local',
    enabled: true,
    createdAt: 1,
    updatedAt: 1,
  })
  store.saveResult(
    'root',
    '/test/physics.txt',
    { contentHash: 'physics', byteSize: 10, modifiedAt: 1 },
    'smart',
    {
      sourceId: 'physics',
      sourceHash: 'physics',
      extractor: { id: 'test', version: '1' },
      textEmbeddingModel: FEATURE_EMBEDDING_MODEL,
      pages: [{ extractedText: 'Charged solar particles', pageNumber: 1 }],
      chunks: [
        {
          id: 'physics-chunk',
          text: 'Charged solar particles',
          embedding: embedText('Charged solar particles'),
        },
      ],
      images: [],
      warnings: [],
      completedCapabilities: ['text-embeddings'],
    }
  )
  const vector = [1, ...Array(255).fill(0)]
  const runtime = {
    snapshot: () => ({
      supported: true,
      modelState: 'ready',
      downloadProgress: 1,
      downloadBytes: 1,
      downloadTotalBytes: 1,
    }),
    embedQuery: vi.fn(async () => vector),
    embedDocuments: vi.fn(async (chunks: unknown[]) => chunks.map(() => vector)),
    shutdown: vi.fn(),
  }
  let roots = ['root']
  const semantic = new SemanticSearch(
    store,
    {
      enabled: () => true,
      roots: () => roots,
      allRoots: () => roots,
      canIndex: () => true,
      waitForIdle: async () => {},
    },
    runtime as unknown as GemmaRuntime
  )
  return {
    store,
    semantic,
    runtime,
    vector,
    disableRoot: () => {
      roots = []
    },
  }
}

describe('optional semantic search lifecycle', () => {
  it('discards a paused in-flight batch and resumes from the persisted index', async () => {
    const { store, semantic, runtime, vector } = fixture()
    let finish: (vectors: number[][]) => void = () => {}
    runtime.embedDocuments.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        })
    )
    try {
      semantic.schedule(0)
      await vi.waitFor(() => expect(runtime.embedDocuments).toHaveBeenCalledTimes(1))
      const paused = semantic.pause()
      finish([vector])
      await paused
      expect(store.getGemmaIndex().counts(['root']).indexedChunks).toBe(0)
      semantic.schedule(0)
      await vi.waitFor(() => expect(store.getGemmaIndex().counts(['root']).indexedChunks).toBe(1))
      await semantic.pause()
    } finally {
      semantic.shutdown()
      store.close()
    }
  })
  it('embeds existing chunks, resumes without repeating work, and caches queries', async () => {
    const { store, semantic, runtime } = fixture()
    try {
      expect(await semantic.search('aurora', 5, ['root'])).toHaveLength(0)
      semantic.schedule(0)
      await vi.waitFor(() => expect(store.getGemmaIndex().counts(['root']).indexedChunks).toBe(1))
      await semantic.pause()
      expect((await semantic.search('aurora', 5, ['root']))[0].sourceId).toBe('physics')
      await semantic.search('aurora', 5, ['root'])
      expect(runtime.embedQuery).toHaveBeenCalledTimes(1)
      semantic.schedule(0)
      await new Promise((resolve) => setTimeout(resolve, 20))
      await semantic.pause()
      expect(runtime.embedDocuments).toHaveBeenCalledTimes(1)
    } finally {
      semantic.shutdown()
      store.close()
    }
  })

  it('keeps keyword search working when inference fails', async () => {
    const { store, semantic, runtime, vector } = fixture()
    try {
      store.getGemmaIndex().save(store.getGemmaIndex().pending(['root']), [vector])
      runtime.embedQuery.mockRejectedValueOnce(new Error('runtime unavailable'))
      expect((await semantic.search('solar', 5, ['root']))[0].sourceId).toBe('physics')
      expect(semantic.snapshot().error).toBe('runtime unavailable')
    } finally {
      semantic.shutdown()
      store.close()
    }
  })

  it('does not return content from a folder disabled during inference', async () => {
    const { store, semantic, runtime, vector, disableRoot } = fixture()
    try {
      store.getGemmaIndex().save(store.getGemmaIndex().pending(['root']), [vector])
      runtime.embedQuery.mockImplementationOnce(async () => {
        disableRoot()
        return vector
      })
      expect(await semantic.search('solar', 5, ['root'])).toHaveLength(0)
      expect(await semantic.search('solar', 0, ['root'])).toHaveLength(0)
    } finally {
      semantic.shutdown()
      store.close()
    }
  })
})
