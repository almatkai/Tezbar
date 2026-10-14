import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { dirname, extname, join } from 'node:path'
import { app } from '@tezbar/desktop-runtime'
import type { BackgroundTask } from '../shared/backgroundTasks'
import type { KnowledgeSnapshot, KnowledgeStatus } from '../shared/knowledge'
import { getKnowledgeSnapshotIfInitialized } from './knowledge/service'

type TimerFileContents = {
  name?: unknown
  pid?: unknown
  pauseElapsed?: unknown
}

function clampProgress(value: number): number {
  return Math.max(0, Math.min(1, value))
}

export function indexingBackgroundTask(status: KnowledgeStatus): BackgroundTask | null {
  if (status.state !== 'scanning' && status.state !== 'indexing') return null

  return {
    id: `indexing:${status.jobId ?? 'active'}`,
    kind: 'indexing',
    title: 'Indexing…',
    detail:
      status.detail ?? (status.state === 'scanning' ? 'Scanning folders' : 'Building local index'),
    progress: clampProgress(status.progress),
  }
}

export function semanticDownloadingBackgroundTask(
  knowledge: KnowledgeSnapshot
): BackgroundTask | null {
  const semantic = knowledge.semanticSearch
  if (!semantic || semantic.modelState !== 'downloading') return null

  const downloadedMb = (semantic.downloadBytes / 1_000_000).toFixed(1)
  const totalMb =
    semantic.downloadTotalBytes > 0
      ? (semantic.downloadTotalBytes / 1_000_000).toFixed(1)
      : null

  return {
    id: 'semantic-downloading:active',
    kind: 'indexing',
    title: 'Downloading model…',
    detail: totalMb
      ? `EmbeddingGemma 2 · ${downloadedMb} / ${totalMb} MB`
      : `EmbeddingGemma 2 · ${downloadedMb} MB`,
    progress: clampProgress(semantic.downloadProgress),
  }
}

export function semanticIndexingBackgroundTask(
  knowledge: KnowledgeSnapshot
): BackgroundTask | null {
  const semantic = knowledge.semanticSearch
  if (!semantic) return null
  if (knowledge.settings.embeddingModel !== 'embeddinggemma-2') return null
  if (semantic.modelState !== 'ready') return null
  if (knowledge.status.state === 'paused' || semantic.error) return null

  const isIndexing =
    semantic.indexing ||
    (semantic.totalChunks > 0 && semantic.indexedChunks < semantic.totalChunks)
  if (!isIndexing) return null
  if (semantic.totalChunks > 0 && semantic.indexedChunks >= semantic.totalChunks) return null

  const progress =
    semantic.totalChunks > 0
      ? clampProgress(semantic.indexedChunks / semantic.totalChunks)
      : undefined
  const detail =
    semantic.totalChunks > 0
      ? `${semantic.indexedChunks.toLocaleString()} / ${semantic.totalChunks.toLocaleString()} chunks`
      : 'Preparing semantic index…'

  return {
    id: 'semantic-indexing:active',
    kind: 'indexing',
    title: 'Semantic indexing…',
    detail,
    progress,
  }
}

export function timerBackgroundTask(
  fileName: string,
  rawContents: string,
  now = Date.now()
): BackgroundTask | null {
  const match = /^(.*?)---(\d+(?:\.\d+)?)\.timer$/.exec(fileName)
  if (!match) return null

  const startedAt = Date.parse(match[1].replace(/__/g, ':'))
  const durationSeconds = Number(match[2])
  if (!Number.isFinite(startedAt) || !Number.isFinite(durationSeconds) || durationSeconds <= 0) {
    return null
  }

  let contents: TimerFileContents
  try {
    contents = JSON.parse(rawContents) as TimerFileContents
  } catch {
    return null
  }

  // The Timers extension removes `pid` when a timer is paused. Paused timers
  // are persisted state, not work currently running in the background.
  if (typeof contents.pid !== 'number' || !Number.isFinite(contents.pid)) return null

  const pauseElapsed =
    typeof contents.pauseElapsed === 'number' && Number.isFinite(contents.pauseElapsed)
      ? Math.max(0, contents.pauseElapsed)
      : 0
  const endsAt = startedAt + (durationSeconds + pauseElapsed) * 1_000
  const remainingSeconds = Math.max(0, Math.ceil((endsAt - now) / 1_000))
  const name =
    typeof contents.name === 'string' && contents.name.trim() ? contents.name.trim() : 'Timer'

  return {
    id: `timer:${fileName}`,
    kind: 'timer',
    title: name,
    detail: remainingSeconds > 0 ? 'Timer running' : 'Timer complete',
    startedAt,
    remainingSeconds,
    extensionId: 'timers',
    commandName: 'manageTimers',
  }
}

function listRunningTimers(now = Date.now()): BackgroundTask[] {
  const userData = app.getPath('userData')
  const packageJsonPath = [
    join(userData, 'extensions', 'timers', 'package.json'),
    join(userData, 'extensions', 'raycast.timers', 'package.json'),
    join(userData, 'extensions', 'packages', 'raycast.timers', 'package.json'),
    join(userData, 'extension-registry', 'packages', 'raycast.timers', 'package.json'),
  ].find((candidate) => existsSync(candidate))
  if (!packageJsonPath) return []

  const supportPath = join(dirname(packageJsonPath), '.tezbar-support')
  if (!existsSync(supportPath)) return []

  const tasks: BackgroundTask[] = []
  for (const fileName of readdirSync(supportPath)) {
    if (extname(fileName) !== '.timer') continue
    try {
      const task = timerBackgroundTask(
        fileName,
        readFileSync(join(supportPath, fileName), 'utf8'),
        now
      )
      if (task) tasks.push(task)
    } catch {
      // A timer can disappear between the directory read and file read when it
      // completes. The next refresh will naturally remove it from the UI.
    }
  }

  return tasks.sort((left, right) => (right.startedAt ?? 0) - (left.startedAt ?? 0))
}

export function listBackgroundTasks(now = Date.now()): BackgroundTask[] {
  const tasks: BackgroundTask[] = []
  const knowledge = getKnowledgeSnapshotIfInitialized()
  if (knowledge) {
    const indexing = indexingBackgroundTask(knowledge.status)
    if (indexing) tasks.push(indexing)

    const semanticDownloading = semanticDownloadingBackgroundTask(knowledge)
    if (semanticDownloading) tasks.push(semanticDownloading)

    const semanticIndexing = semanticIndexingBackgroundTask(knowledge)
    if (semanticIndexing) tasks.push(semanticIndexing)
  }
  tasks.push(...listRunningTimers(now))
  return tasks
}
