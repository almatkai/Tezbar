import { describe, expect, it } from 'vitest'
import {
  indexingBackgroundTask,
  semanticDownloadingBackgroundTask,
  semanticIndexingBackgroundTask,
  timerBackgroundTask,
} from './backgroundTasks'
import type { KnowledgeSnapshot, KnowledgeStatus } from '../shared/knowledge'

function knowledgeStatus(state: KnowledgeStatus['state'], progress = 0.42): KnowledgeStatus {
  return {
    state,
    backend: 'local',
    jobId: 'job-1',
    progress,
    detail: 'Indexing Documents',
    queuedSources: 3,
    processedSources: 1,
    failedSources: 0,
    sourceCount: 4,
    chunkCount: 20,
    indexedPageCount: 2,
    totalPageCount: 5,
    partialSourceCount: 0,
    sourceBytes: 100,
  }
}

function knowledgeSnapshot(overrides: Partial<KnowledgeSnapshot> = {}): KnowledgeSnapshot {
  return {
    roots: [],
    status: knowledgeStatus('idle'),
    localBackendAvailable: true,
    cloudBackendAvailable: false,
    settings: {
      depth: 'smart',
      embeddingModel: 'embeddinggemma-2',
      indexingSpeed: 'medium',
      runOnBattery: true,
      onlyRunHeavyJobsWhenIdle: false,
      maxConcurrentExtractors: 2,
      maxConcurrentOcrJobs: 2,
    },
    storageBytes: 1024,
    semanticSearch: {
      supported: true,
      modelState: 'ready',
      downloadProgress: 1,
      downloadBytes: 322_000_000,
      downloadTotalBytes: 322_000_000,
      indexing: true,
      indexedChunks: 129_431,
      totalChunks: 547_830,
    },
    ...overrides,
  }
}

describe('background tasks', () => {
  it('exposes active indexing with normalized progress', () => {
    expect(indexingBackgroundTask(knowledgeStatus('indexing', 1.4))).toMatchObject({
      kind: 'indexing',
      title: 'Indexing…',
      progress: 1,
    })
    expect(indexingBackgroundTask(knowledgeStatus('completed'))).toBeNull()
  })

  it('reads a running timer from the Timers extension file format', () => {
    const now = Date.parse('2026-07-13T02:10:00.000Z')
    const task = timerBackgroundTask(
      '2026-07-13T02__00__00.000Z---900.timer',
      JSON.stringify({ name: 'Deep work', pid: 123, pauseElapsed: 0 }),
      now
    )

    expect(task).toMatchObject({
      kind: 'timer',
      title: 'Deep work',
      remainingSeconds: 300,
      extensionId: 'timers',
      commandName: 'manageTimers',
    })
  })

  it('does not report paused timers as running work', () => {
    expect(
      timerBackgroundTask(
        '2026-07-13T02__00__00.000Z---900.timer',
        JSON.stringify({ name: 'Paused', pauseElapsed: 0 })
      )
    ).toBeNull()
  })

  it('exposes active semantic indexing and chunking progress', () => {
    const snapshot = knowledgeSnapshot()
    const task = semanticIndexingBackgroundTask(snapshot)

    expect(task).toMatchObject({
      id: 'semantic-indexing:active',
      kind: 'indexing',
      title: 'Semantic indexing…',
      detail: '129,431 / 547,830 chunks',
    })
    expect(task?.progress).toBeCloseTo(129_431 / 547_830, 4)
  })

  it('does not expose semantic indexing when model is not embeddinggemma-2 or paused', () => {
    expect(
      semanticIndexingBackgroundTask(
        knowledgeSnapshot({
          settings: {
            depth: 'smart',
            embeddingModel: 'built-in',
            indexingSpeed: 'medium',
            runOnBattery: true,
            onlyRunHeavyJobsWhenIdle: false,
            maxConcurrentExtractors: 2,
            maxConcurrentOcrJobs: 2,
          },
        })
      )
    ).toBeNull()

    expect(
      semanticIndexingBackgroundTask(
        knowledgeSnapshot({
          status: knowledgeStatus('paused'),
        })
      )
    ).toBeNull()

    expect(
      semanticIndexingBackgroundTask(
        knowledgeSnapshot({
          semanticSearch: {
            supported: true,
            modelState: 'ready',
            downloadProgress: 1,
            downloadBytes: 322_000_000,
            downloadTotalBytes: 322_000_000,
            indexing: false,
            indexedChunks: 547_830,
            totalChunks: 547_830,
          },
        })
      )
    ).toBeNull()
  })

  it('exposes semantic model downloading progress', () => {
    const task = semanticDownloadingBackgroundTask(
      knowledgeSnapshot({
        semanticSearch: {
          supported: true,
          modelState: 'downloading',
          downloadProgress: 0.5,
          downloadBytes: 161_000_000,
          downloadTotalBytes: 322_000_000,
          indexing: false,
          indexedChunks: 0,
          totalChunks: 0,
        },
      })
    )

    expect(task).toMatchObject({
      id: 'semantic-downloading:active',
      kind: 'indexing',
      title: 'Downloading model…',
      detail: 'EmbeddingGemma 2 · 161.0 / 322.0 MB',
      progress: 0.5,
    })
  })
})
