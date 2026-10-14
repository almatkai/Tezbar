import { randomUUID } from 'node:crypto'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { describe, expect, it } from 'vitest'
import { KnowledgeStore } from '../database/store'
import { embedText } from './featureEmbedding'
import { FEATURE_EMBEDDING_MODEL } from './featureEmbedding'
import { fuseKnowledgeHits } from './gemmaIndex'

function fixture() {
  process.env.APPDATA_DIR = join(tmpdir(), `tezbar-gemma-index-${randomUUID()}`)
  const store = new KnowledgeStore()
  for (const rootId of ['root-a', 'root-b']) {
    store.upsertRoot({
      id: rootId,
      path: `/test/${rootId}`,
      depth: 'smart',
      processingBackend: 'local',
      enabled: true,
      createdAt: 1,
      updatedAt: 1,
    })
    store.saveResult(
      rootId,
      `/test/${rootId}/physics.txt`,
      { contentHash: rootId, byteSize: 100, modifiedAt: 1 },
      'smart',
      {
        sourceId: rootId,
        sourceHash: rootId,
        extractor: { id: 'test', version: '1' },
        textEmbeddingModel: FEATURE_EMBEDDING_MODEL,
        pages: [{ pageNumber: 1, extractedText: 'charged particles' }],
        chunks: [
          {
            id: `${rootId}-chunk`,
            text: 'charged particles',
            embedding: embedText('charged particles'),
          },
        ],
        images: [],
        warnings: [],
        completedCapabilities: ['text-embeddings'],
      }
    )
  }
  return store
}

describe('Gemma vector index', () => {
  it('isolates the learned vector space, respects folders, and removes obsolete vectors', () => {
    const store = fixture()
    const index = store.getGemmaIndex()
    const vector = [1, ...Array(255).fill(0)]
    expect(index.counts(['root-a'])).toEqual({ indexedChunks: 0, totalChunks: 1 })
    index.save(index.pending(['root-a', 'root-b']), [vector, vector])
    expect(index.counts(['root-a'])).toEqual({ indexedChunks: 1, totalChunks: 1 })
    expect(index.search(vector, ['root-a'], 5)).toEqual([
      expect.objectContaining({ sourceId: 'root-a', semanticScore: 1 }),
    ])
    expect(index.pending(['root-a'])).toHaveLength(0)
    expect(index.search(vector, [], 5)).toHaveLength(0)
    expect(() => index.search(embedText('charged particles'), ['root-a'], 5)).toThrow()
    store.removeSource('root-a')
    expect(index.search(vector, ['root-a'], 5)).toHaveLength(0)
    expect(index.counts(['root-a'])).toEqual({ indexedChunks: 0, totalChunks: 0 })
    index.clear()
    expect(index.pending(['root-b'])).toHaveLength(1)
  })

  it('does not resurrect chunks removed while inference was in flight', () => {
    const store = fixture()
    const index = store.getGemmaIndex()
    const pending = index.pending(['root-a'])
    store.removeSource('root-a')
    index.save(pending, [[1, ...Array(255).fill(0)]])
    expect(index.counts(['root-a'])).toEqual({ indexedChunks: 0, totalChunks: 0 })
  })

  it('fuses rankings while retaining lexical and semantic evidence', () => {
    const store = fixture()
    const lexical = store.search('charged particles', 5, ['root-a'], true)
    const semantic = [{ ...lexical[0], semanticScore: 0.9, lexicalScore: 0 }]
    const fused = fuseKnowledgeHits(lexical, semantic, 5)
    expect(fused).toHaveLength(1)
    expect(fused[0].semanticScore).toBe(0.9)
    expect(fused[0].lexicalScore).toBeGreaterThan(0)
    expect(fused[0].score).toBeGreaterThan(0.9)
  })
})
