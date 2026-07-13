import { describe, expect, it } from 'vitest'
import { indexingBackgroundTask, timerBackgroundTask } from './backgroundTasks'
import type { KnowledgeStatus } from '../shared/knowledge'

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
})
