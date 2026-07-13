import { randomUUID } from 'node:crypto'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { describe, expect, it } from 'vitest'
import { embedText } from '../embeddings/featureEmbedding'
import { KnowledgeStore } from './store'

describe('knowledge store', () => {
  it('persists backend-neutral artifacts and retrieves content hits', () => {
    process.env.APPDATA_DIR = join(tmpdir(), `tezbar-knowledge-test-${randomUUID()}`)
    const store = new KnowledgeStore()
    const now = Date.now()
    store.upsertRoot({
      id: 'root-1',
      path: '/tmp/knowledge',
      depth: 'inherit',
      processingBackend: 'local',
      enabled: true,
      createdAt: now,
      updatedAt: now,
    })

    const fingerprint = { contentHash: 'hash-1', byteSize: 42, modifiedAt: now }
    store.saveResult('root-1', '/tmp/knowledge/certificate.txt', fingerprint, 'smart-profile', {
      sourceId: 'source-1',
      sourceHash: fingerprint.contentHash,
      extractor: { id: 'test', version: '1' },
      textEmbeddingModel: { id: 'test-vector', version: '1', dimensions: 384 },
      pages: [{ pageNumber: 1, extractedText: 'The signing certificate expired yesterday.' }],
      chunks: [
        {
          id: 'chunk-1',
          pageNumber: 1,
          text: 'The signing certificate expired yesterday.',
          embedding: embedText('The signing certificate expired yesterday.'),
        },
      ],
      images: [],
      completedCapabilities: ['extracted-text', 'chunks', 'text-embeddings'],
      warnings: [],
    })

    const hits = store.search('expired certificate', 5)
    expect(hits[0]).toMatchObject({
      sourceId: 'source-1',
      title: 'certificate.txt',
      pageNumber: 1,
    })
    expect(store.counts()).toEqual({
      sourceCount: 1,
      chunkCount: 1,
      indexedPageCount: 1,
      totalPageCount: 1,
      partialSourceCount: 0,
      sourceBytes: 42,
    })
    expect(store.listSources({ query: 'certificate', limit: 10 })).toMatchObject({
      total: 1,
      hasMore: false,
      sources: [expect.objectContaining({ title: 'certificate.txt', byteSize: 42 })],
    })
    expect(store.readChunk('chunk-1', 2_000, ['root-1'])).toMatchObject({
      path: '/tmp/knowledge/certificate.txt',
      pageNumber: 1,
    })
    expect(store.readChunk('chunk-1', 2_000, ['different-root'])).toBeNull()

    const reused = store.findReusableResult('hash-1', 'source-2', 'smart-profile')
    expect(reused).toMatchObject({
      sourceId: 'source-2',
      sourceHash: 'hash-1',
      extractor: { id: 'content-addressed-cache' },
    })
    expect(reused?.chunks[0]?.id).not.toBe('chunk-1')
    expect(reused?.chunks[0]?.text).toContain('certificate expired')
    expect(store.findReusableResult('hash-1', 'source-3', 'basic-profile')).toBeNull()
    expect(store.getSettings().depth).toBe('smart')

    store.saveIndexingCheckpoint('job-1', [
      { rootId: 'root-1', path: '/tmp/knowledge/first.txt' },
      { rootId: 'root-1', path: '/tmp/knowledge/second.txt' },
    ])
    expect(store.getIndexingCheckpoint()).toEqual({
      jobId: 'job-1',
      totalSources: 2,
      processedSources: 0,
      failedSources: 0,
      candidates: [
        { position: 0, rootId: 'root-1', path: '/tmp/knowledge/first.txt' },
        { position: 1, rootId: 'root-1', path: '/tmp/knowledge/second.txt' },
      ],
    })
    expect(store.completeIndexingCandidate('job-1', 0, false)).toBe(true)
    expect(store.completeIndexingCandidate('job-1', 0, false)).toBe(false)
    expect(new KnowledgeStore().getIndexingCheckpoint()).toMatchObject({
      jobId: 'job-1',
      totalSources: 2,
      processedSources: 1,
      failedSources: 0,
      candidates: [{ position: 1, path: '/tmp/knowledge/second.txt' }],
    })
    expect(store.completeIndexingCandidate('job-1', 1, true)).toBe(true)
    expect(store.getIndexingCheckpoint()).toMatchObject({
      processedSources: 2,
      failedSources: 1,
      candidates: [],
    })
    store.clearIndexingCheckpoint('job-1')
    expect(store.getIndexingCheckpoint()).toBeNull()

    const replacementFingerprint = { contentHash: 'hash-2', byteSize: 84, modifiedAt: now + 1 }
    store.saveResult(
      'root-1',
      '/tmp/knowledge/certificate.txt',
      replacementFingerprint,
      'smart-profile',
      {
        sourceId: 'source-1',
        sourceHash: replacementFingerprint.contentHash,
        extractor: { id: 'test', version: '1' },
        pages: [
          { pageNumber: 1, extractedText: 'Replacement page one.' },
          { pageNumber: 2, extractedText: 'Replacement page two.' },
        ],
        chunks: [
          { id: 'chunk-2', pageNumber: 1, text: 'Replacement page one.' },
          { id: 'chunk-3', pageNumber: 2, text: 'Replacement page two.' },
        ],
        images: [],
        completedCapabilities: ['extracted-text', 'chunks'],
        warnings: [],
        totalPageCount: 3,
      }
    )
    expect(store.counts()).toEqual({
      sourceCount: 1,
      chunkCount: 2,
      indexedPageCount: 2,
      totalPageCount: 3,
      partialSourceCount: 1,
      sourceBytes: 84,
    })
    expect(store.search('replacement page', 5)[0]?.text).toContain('Replacement page')
    expect(store.search('signing certificate expired', 5)).toHaveLength(0)

    store.removeRoot('root-1')
    expect(store.counts()).toEqual({
      sourceCount: 0,
      chunkCount: 0,
      indexedPageCount: 0,
      totalPageCount: 0,
      partialSourceCount: 0,
      sourceBytes: 0,
    })
  })
})
