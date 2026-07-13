import { execFile } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { existsSync, readdirSync, statSync, watch, type FSWatcher } from 'node:fs'
import { homedir } from 'node:os'
import { basename, extname, join, relative, resolve, sep } from 'node:path'
import { promisify } from 'node:util'
import type {
  IndexingBackend,
  IndexingRequest,
  KnowledgeDepth,
  KnowledgeRoot,
  KnowledgeRootDepth,
  KnowledgeReadResult,
  KnowledgeSearchHit,
  KnowledgeSettings,
  KnowledgeSnapshot,
  KnowledgeSourcesPage,
  KnowledgeStatus,
} from '../../shared/knowledge'
import { LocalIndexingBackend } from './backends/local/backend'
import { TezbarCloudIndexingBackend } from './backends/cloud/backend'
import { fingerprintSource, sourceIdForPath } from './core/fingerprint'
import { getKnowledgeStore, type IndexingCheckpoint } from './database/store'
import { isIndexablePath, maximumIndexableSourceBytes } from './extractors/localExtractor'
import {
  DEFAULT_KNOWLEDGE_SETTINGS,
  effectiveKnowledgeDepth,
  indexingProfileKey,
  profileForDepth,
} from './depth'
import { KnowledgeWorkerHost } from './workerHost'

const execFileAsync = promisify(execFile)
const MAX_SCANNED_FILES = 75_000
const STATUS_EVENT_INTERVAL_MS = 100
const STATUS_PERSIST_INTERVAL_MS = 1_000
const BACKGROUND_FILE_DELAY_MS = 12
const SKIP_NAMES = new Set([
  '.git',
  '.svn',
  '.hg',
  'Library',
  'Applications',
  'System',
  'AppData',
  'Windows',
  'Program Files',
  'Program Files (x86)',
  'ProgramData',
  '$RECYCLE.BIN',
  'node_modules',
  'bower_components',
  'Pods',
  'DerivedData',
  '.next',
  '.nuxt',
  '.cache',
  '.idea',
  '.vscode',
  '.gradle',
  '.terraform',
  '.serverless',
  'build',
  'dist',
  'coverage',
  'out',
  'target',
  '__pycache__',
  '.venv',
  'venv',
])

const SKIP_FILE_NAMES = new Set([
  '.DS_Store',
  'Thumbs.db',
  'desktop.ini',
  'package-lock.json',
  'pnpm-lock.yaml',
  'yarn.lock',
  'bun.lock',
  'bun.lockb',
  'Cargo.lock',
  'composer.lock',
  'Gemfile.lock',
  'Pipfile.lock',
  'poetry.lock',
])

const SENSITIVE_FILE_PATTERNS = [
  /^(?:id_(?:rsa|dsa|ecdsa|ed25519)|authorized_keys|known_hosts)$/i,
  /(?:^|[._-])(?:credential|credentials|secret|secrets|token|tokens)(?:[._-]|$)/i,
  /\.(?:pem|key|p12|pfx|keystore)$/i,
]

const SKIP_DIRECTORY_SUFFIXES = [
  '.app',
  '.bundle',
  '.framework',
  '.photoslibrary',
  '.photolibrary',
  '.plugin',
  '.xcarchive',
]

const MAJOR_KNOWLEDGE_FOLDER_NAMES = ['Desktop', 'Documents', 'Downloads', 'Pictures'] as const

export function shouldSkipKnowledgeEntry(name: string, isDirectory: boolean): boolean {
  if (name.startsWith('.') || SKIP_NAMES.has(name)) return true
  const lower = name.toLowerCase()
  if (isDirectory) return SKIP_DIRECTORY_SUFFIXES.some((suffix) => lower.endsWith(suffix))
  if (SKIP_FILE_NAMES.has(name)) return true
  if (SENSITIVE_FILE_PATTERNS.some((pattern) => pattern.test(name))) return true
  return lower.endsWith('.min.js') || lower.endsWith('.min.css')
}

export function discoverMajorKnowledgeFolders(home = homedir()): string[] {
  return MAJOR_KNOWLEDGE_FOLDER_NAMES.map((name) => join(home, name)).filter((path) => {
    try {
      return statSync(path).isDirectory()
    } catch {
      return false
    }
  })
}

export function isKnowledgeCandidatePath(rootPath: string, path: string): boolean {
  const relativePath = relative(rootPath, path)
  if (!relativePath || relativePath === '..' || relativePath.startsWith(`..${sep}`)) return false
  const segments = relativePath.split(sep).filter(Boolean)
  const fileName = segments.pop()
  if (!fileName || shouldSkipKnowledgeEntry(fileName, false) || !isIndexablePath(path)) return false
  return segments.every((name) => !shouldSkipKnowledgeEntry(name, true))
}

function initialStatus(): KnowledgeStatus {
  return {
    state: 'idle',
    backend: 'local',
    progress: 0,
    queuedSources: 0,
    processedSources: 0,
    failedSources: 0,
    sourceCount: 0,
    chunkCount: 0,
    indexedPageCount: 0,
    totalPageCount: 0,
    partialSourceCount: 0,
    sourceBytes: 0,
  }
}

function emitStatus(status: KnowledgeStatus): void {
  process.stdout.write(
    `${JSON.stringify({ type: 'event', channel: 'knowledge:status', payload: status })}\n`
  )
}

type Candidate = { root: KnowledgeRoot; path: string }
type QueuedCandidate = Candidate & { position: number }
type KnowledgeServiceMode = 'inline' | 'coordinator' | 'worker'

type KnowledgeServiceOptions = {
  mode?: KnowledgeServiceMode
  statusSink?: (status: KnowledgeStatus) => void
}

export class KnowledgeService {
  private readonly store = getKnowledgeStore()
  private readonly backends = new Map<string, IndexingBackend>([
    ['local', new LocalIndexingBackend()],
    ['tezbar-cloud', new TezbarCloudIndexingBackend()],
  ])
  private status: KnowledgeStatus = initialStatus()
  private settings: KnowledgeSettings = { ...DEFAULT_KNOWLEDGE_SETTINGS }
  private controller: AbortController | null = null
  private activePromise: Promise<void> | null = null
  private initialized = false
  private readonly watchers = new Map<string, FSWatcher>()
  private rescanTimer: NodeJS.Timeout | null = null
  private startupTimer: NodeJS.Timeout | null = null
  private rescanRequested = false
  private manuallyPaused = false
  private interactiveUntil = 0
  private lastStatusEventAt = 0
  private lastStatusPersistAt = 0
  private readonly mode: KnowledgeServiceMode
  private readonly statusSink: (status: KnowledgeStatus) => void
  private readonly workerHost: KnowledgeWorkerHost | null

  constructor(options: KnowledgeServiceOptions = {}) {
    this.mode = options.mode ?? 'inline'
    this.statusSink = options.statusSink ?? emitStatus
    this.workerHost =
      this.mode === 'coordinator'
        ? new KnowledgeWorkerHost(
            (status) => this.acceptWorkerStatus(status),
            (exit) => this.handleWorkerExit(exit),
          )
        : null
  }

  initialize(): void {
    if (this.initialized) return
    this.store.ensureInitialized()
    this.settings = this.store.getSettings()
    this.store.saveSettings(this.settings)
    this.status = this.store.getPersistedStatus()
    this.manuallyPaused = this.status.state === 'paused'
    const checkpoint = this.store.getIndexingCheckpoint()
    if (checkpoint && checkpoint.candidates.length > 0) {
      const processed = Math.min(checkpoint.processedSources, checkpoint.totalSources)
      this.status = {
        ...this.status,
        state: this.manuallyPaused ? 'paused' : 'indexing',
        jobId: checkpoint.jobId,
        progress: checkpoint.totalSources > 0 ? processed / checkpoint.totalSources : 1,
        queuedSources: checkpoint.candidates.length,
        processedSources: processed,
        failedSources: checkpoint.failedSources,
        detail: this.manuallyPaused
          ? `Indexing paused · ${checkpoint.candidates.length} files remaining`
          : `Ready to resume ${checkpoint.candidates.length} files`,
      }
      this.store.saveStatus(this.status)
    } else if (checkpoint) {
      this.status = {
        ...this.status,
        state: 'completed',
        progress: 1,
        queuedSources: 0,
        processedSources: checkpoint.processedSources,
        failedSources: checkpoint.failedSources,
        detail: `Indexed ${checkpoint.processedSources} files`,
        lastCompletedAt: this.status.lastCompletedAt ?? Date.now(),
      }
      this.store.clearIndexingCheckpoint(checkpoint.jobId)
      this.store.saveStatus(this.status)
    } else if (this.status.state === 'scanning' || this.status.state === 'indexing') {
      this.status = { ...this.status, state: 'idle', progress: 0, detail: undefined }
      this.store.saveStatus(this.status)
    }
    this.refreshCounts()
    this.initialized = true
    if (this.mode !== 'worker') this.syncWatchers()
    if (this.mode !== 'worker' && !this.manuallyPaused && this.activeRoots().length > 0) {
      this.startupTimer = setTimeout(() => {
        this.startupTimer = null
        void this.startIndexing()
      }, 2_500)
      this.startupTimer.unref()
    }
  }

  snapshot(): KnowledgeSnapshot {
    this.initialize()
    this.refreshCounts()
    return {
      roots: this.store.listRoots(),
      status: { ...this.status },
      localBackendAvailable: true,
      cloudBackendAvailable: false,
      settings: { ...this.settings },
      storageBytes: this.store.storageBytes(),
    }
  }

  addRoot(path: string): KnowledgeSnapshot {
    this.initialize()
    const normalized = resolve(path.trim())
    if (!normalized || !existsSync(normalized) || !statSync(normalized).isDirectory()) {
      throw new Error('Choose an existing folder')
    }
    const current = this.store.listRoots()
    const duplicate = current.find((root) => root.path === normalized)
    if (duplicate) return this.snapshot()
    const now = Date.now()
    this.store.upsertRoot({
      id: randomUUID(),
      path: normalized,
      depth: 'inherit',
      processingBackend: 'local',
      enabled: true,
      createdAt: now,
      updatedAt: now,
    })
    this.syncWatchers()
    if (this.isIndexingActive()) this.rescanRequested = true
    void this.startIndexing()
    return this.snapshot()
  }

  addMajorRoots(): KnowledgeSnapshot {
    this.initialize()
    const existingRoots = new Map(this.store.listRoots().map((root) => [root.path, root]))
    const now = Date.now()
    for (const path of discoverMajorKnowledgeFolders()) {
      const existing = existingRoots.get(path)
      if (existing) {
        if (!existing.enabled) this.store.upsertRoot({ ...existing, enabled: true, updatedAt: now })
        continue
      }
      this.store.upsertRoot({
        id: randomUUID(),
        path,
        depth: 'inherit',
        processingBackend: 'local',
        enabled: true,
        createdAt: now,
        updatedAt: now,
      })
    }
    this.syncWatchers()
    if (this.isIndexingActive()) this.rescanRequested = true
    void this.startIndexing()
    return this.snapshot()
  }

  async removeRoot(rootId: string): Promise<KnowledgeSnapshot> {
    this.initialize()
    await this.stopCoordinatorWorker()
    this.store.removeRoot(rootId)
    this.store.clearIndexingCheckpoint()
    this.syncWatchers()
    this.refreshCounts()
    this.persistStatus()
    if (!this.manuallyPaused && this.activeRoots().length > 0) void this.startIndexing()
    return this.snapshot()
  }

  async setRootEnabled(rootId: string, enabled: boolean): Promise<KnowledgeSnapshot> {
    this.initialize()
    const root = this.store.listRoots().find((candidate) => candidate.id === rootId)
    if (!root) throw new Error('Knowledge folder was not found')
    await this.stopCoordinatorWorker()
    this.store.upsertRoot({ ...root, enabled, updatedAt: Date.now() })
    this.store.clearIndexingCheckpoint()
    this.syncWatchers()
    if (!this.manuallyPaused && this.activeRoots().length > 0) void this.startIndexing()
    return this.snapshot()
  }

  async setDepth(depth: KnowledgeDepth): Promise<KnowledgeSnapshot> {
    this.initialize()
    if (!['off', 'basic', 'smart', 'deep'].includes(depth)) {
      throw new Error('Invalid Knowledge Depth')
    }
    if (this.settings.depth === depth) return this.snapshot()
    this.settings = { ...this.settings, depth }
    this.store.saveSettings(this.settings)
    return this.restartForConfigurationChange(`Knowledge Depth changed to ${depth}`)
  }

  async setRootDepth(rootId: string, depth: KnowledgeRootDepth): Promise<KnowledgeSnapshot> {
    this.initialize()
    if (!['inherit', 'off', 'basic', 'smart', 'deep'].includes(depth)) {
      throw new Error('Invalid folder Knowledge Depth')
    }
    const root = this.store.listRoots().find((candidate) => candidate.id === rootId)
    if (!root) throw new Error('Knowledge folder was not found')
    if (root.depth === depth) return this.snapshot()
    this.store.upsertRoot({ ...root, depth, updatedAt: Date.now() })
    this.syncWatchers()
    return this.restartForConfigurationChange('Folder Knowledge Depth changed')
  }

  search(query: string, limit = 12): KnowledgeSearchHit[] {
    this.initialize()
    return this.store.search(
      query,
      limit,
      this.activeRoots().map((root) => root.id)
    )
  }

  read(resultId: string, maxChars = 12_000): KnowledgeReadResult | null {
    this.initialize()
    return this.store.readChunk(
      resultId,
      maxChars,
      this.activeRoots().map((root) => root.id)
    )
  }

  listSources(input?: { query?: string; offset?: number; limit?: number }): KnowledgeSourcesPage {
    this.initialize()
    return this.store.listSources(input)
  }

  notifyInteractiveActivity(durationMs = 1_000): void {
    this.interactiveUntil = Math.max(this.interactiveUntil, Date.now() + durationMs)
  }

  async startIndexing(): Promise<KnowledgeSnapshot> {
    this.initialize()
    if (this.startupTimer) clearTimeout(this.startupTimer)
    this.startupTimer = null
    if (this.manuallyPaused) return this.snapshot()
    if (this.isIndexingActive()) return this.snapshot()
    const roots = this.activeRoots()
    if (roots.length === 0) return this.snapshot()
    const checkpoint = this.store.getIndexingCheckpoint()
    const jobId = checkpoint?.jobId ?? randomUUID()
    if (checkpoint) {
      const processed = Math.min(checkpoint.processedSources, checkpoint.totalSources)
      this.updateStatus({
        ...this.status,
        state: 'indexing',
        jobId,
        progress: checkpoint.totalSources > 0 ? processed / checkpoint.totalSources : 1,
        queuedSources: checkpoint.candidates.length,
        processedSources: processed,
        failedSources: checkpoint.failedSources,
        error: undefined,
        detail: `Resuming ${checkpoint.candidates.length} files`,
      })
    } else {
      this.updateStatus({
        ...initialStatus(),
        state: 'scanning',
        backend: 'local',
        jobId,
        detail: 'Scanning knowledge folders',
      })
    }
    if (this.mode === 'coordinator') {
      // A resumed queue may have been created by older scan rules. Reconcile
      // once it drains so newly ignored bundles and deleted files are removed.
      if (checkpoint) this.rescanRequested = true
      this.workerHost?.start()
      return this.snapshot()
    }
    this.controller = new AbortController()
    const run = checkpoint
      ? this.resumeIndexing(checkpoint, roots, this.controller.signal)
      : this.runIndexing(jobId, roots, this.controller.signal)
    this.activePromise = run.finally(() => {
      this.activePromise = null
      this.controller = null
      if (this.rescanRequested && !this.manuallyPaused) {
        this.rescanRequested = false
        this.scheduleRescan()
      }
    })
    void this.activePromise
    return this.snapshot()
  }

  async pause(): Promise<KnowledgeSnapshot> {
    this.manuallyPaused = true
    this.rescanRequested = false
    if (this.rescanTimer) clearTimeout(this.rescanTimer)
    this.rescanTimer = null
    if (this.mode === 'coordinator') {
      await this.workerHost?.stop()
      this.updateStatus({ ...this.status, state: 'paused', detail: 'Indexing paused' })
      return this.snapshot()
    }
    this.controller?.abort()
    const jobId = this.status.jobId
    if (jobId) await this.backends.get('local')?.cancel(jobId)
    this.updateStatus({ ...this.status, state: 'paused', detail: 'Indexing paused' })
    return this.snapshot()
  }

  async resume(): Promise<KnowledgeSnapshot> {
    this.manuallyPaused = false
    this.rescanRequested = false
    if (this.mode === 'coordinator') await this.workerHost?.stop()
    else if (this.activePromise) await this.activePromise.catch(() => {})
    return this.startIndexing()
  }

  async waitForCurrentRun(): Promise<void> {
    const active = this.activePromise
    if (active) await active.catch(() => {})
  }

  private async runIndexing(
    jobId: string,
    roots: KnowledgeRoot[],
    signal: AbortSignal
  ): Promise<void> {
    try {
      const candidates: Candidate[] = []
      for (const root of roots) {
        const scan = await this.scanRoot(root, signal)
        if (scan.complete) this.store.removeMissingSources(root.id, new Set(scan.paths))
        candidates.push(
          ...scan.paths
            .filter((path) => this.needsIndexing(root, path))
            .map((path) => ({ root, path })),
        )
      }
      if (signal.aborted) return
      this.updateStatus({
        ...this.status,
        state: 'indexing',
        queuedSources: candidates.length,
        progress: candidates.length === 0 ? 1 : 0,
        detail:
          candidates.length === 0
            ? 'No supported files found'
            : `Preparing ${candidates.length} files`,
      })
      const checkpoint = this.store.saveIndexingCheckpoint(
        jobId,
        candidates.map((candidate) => ({ rootId: candidate.root.id, path: candidate.path }))
      )
      await this.processCandidateQueue(
        checkpoint,
        candidates.map((candidate, position) => ({ ...candidate, position })),
        signal
      )
    } catch (error) {
      if (signal.aborted) return
      this.updateStatus({
        ...this.status,
        state: 'failed',
        error: error instanceof Error ? error.message : String(error),
        detail: 'Indexing stopped',
      })
    }
  }

  private async resumeIndexing(
    checkpoint: IndexingCheckpoint,
    roots: KnowledgeRoot[],
    signal: AbortSignal
  ): Promise<void> {
    try {
      const rootsById = new Map(roots.map((root) => [root.id, root]))
      const candidates: QueuedCandidate[] = []
      let processed = checkpoint.processedSources
      for (const persisted of checkpoint.candidates) {
        const root = rootsById.get(persisted.rootId)
        if (
          !root ||
          !isKnowledgeCandidatePath(root.path, persisted.path) ||
          !this.needsIndexing(root, persisted.path)
        ) {
          if (this.store.completeIndexingCandidate(checkpoint.jobId, persisted.position, false)) {
            processed += 1
          }
          continue
        }
        candidates.push({ root, path: persisted.path, position: persisted.position })
      }
      await this.processCandidateQueue(
        { ...checkpoint, processedSources: processed },
        candidates,
        signal
      )
    } catch (error) {
      if (signal.aborted) return
      this.updateStatus({
        ...this.status,
        state: 'failed',
        error: error instanceof Error ? error.message : String(error),
        detail: 'Indexing stopped',
      })
    }
  }

  private async processCandidateQueue(
    checkpoint: IndexingCheckpoint,
    candidates: QueuedCandidate[],
    signal: AbortSignal
  ): Promise<void> {
    let cursor = 0
    let completed = checkpoint.processedSources
    let failed = checkpoint.failedSources
    const total = checkpoint.totalSources
    this.updateStatus({
      ...this.status,
      state: 'indexing',
      jobId: checkpoint.jobId,
      progress: total > 0 ? Math.min(1, completed / total) : 1,
      queuedSources: Math.max(0, total - completed),
      processedSources: completed,
      failedSources: failed,
      detail: candidates.length === 0 ? 'Finishing index' : `Indexing ${candidates.length} files`,
    })

    const workers = Array.from(
      { length: Math.min(this.settings.maxConcurrentExtractors, candidates.length) },
      async () => {
        while (!signal.aborted) {
          const index = cursor
          cursor += 1
          const candidate = candidates[index]
          if (!candidate) return
          const succeeded = await this.indexCandidate(
            checkpoint.jobId,
            candidate,
            total,
            () => completed,
            signal
          )
          if (signal.aborted) return
          if (
            !this.store.completeIndexingCandidate(checkpoint.jobId, candidate.position, !succeeded)
          ) {
            continue
          }
          completed += 1
          if (!succeeded) failed += 1
          this.updateStatus({ ...this.status, processedSources: completed, failedSources: failed })
          this.updateProgress(completed, total, `Processed ${basename(candidate.path)}`)
          if (this.mode === 'worker') {
            await new Promise<void>((resolve) => setTimeout(resolve, BACKGROUND_FILE_DELAY_MS))
          }
        }
      }
    )
    await Promise.all(workers)
    if (signal.aborted) return
    this.updateStatus({
      ...this.status,
      state: 'completed',
      progress: 1,
      queuedSources: 0,
      processedSources: completed,
      failedSources: failed,
      detail: `Indexed ${completed} files`,
      lastCompletedAt: Date.now(),
    })
    this.store.clearIndexingCheckpoint(checkpoint.jobId)
  }

  private async scanRoot(
    root: KnowledgeRoot,
    signal: AbortSignal
  ): Promise<{ paths: string[]; complete: boolean }> {
    const queue = [root.path]
    const paths: string[] = []
    let visited = 0
    let complete = true
    while (queue.length > 0 && visited < MAX_SCANNED_FILES && !signal.aborted) {
      const directory = queue.shift()
      if (!directory) break
      let entries
      try {
        entries = readdirSync(directory, { withFileTypes: true })
      } catch {
        complete = false
        continue
      }
      for (const entry of entries) {
        if (signal.aborted) break
        visited += 1
        if (shouldSkipKnowledgeEntry(entry.name, entry.isDirectory())) continue
        const path = join(directory, entry.name)
        if (entry.isSymbolicLink()) continue
        if (entry.isDirectory()) {
          queue.push(path)
        } else if (entry.isFile() && isIndexablePath(path)) {
          try {
            const fileStat = statSync(path)
            const isExtensionlessExecutable = !extname(path) && (fileStat.mode & 0o111) !== 0
            if (!isExtensionlessExecutable && fileStat.size <= maximumIndexableSourceBytes(path)) {
              paths.push(path)
            }
          } catch {
            // File disappeared while scanning.
          }
        }
        if (visited % 300 === 0) await new Promise<void>((resolve) => setImmediate(resolve))
      }
    }
    if (visited >= MAX_SCANNED_FILES || signal.aborted) complete = false
    return { paths, complete }
  }

  private async indexCandidate(
    jobId: string,
    candidate: Candidate,
    total: number,
    completed: () => number,
    signal: AbortSignal
  ): Promise<boolean> {
    const depth = effectiveKnowledgeDepth(candidate.root, this.settings)
    const profile = profileForDepth(depth)
    if (!profile) return true
    const profileKey = indexingProfileKey(profile)
    const sourceId = sourceIdForPath(candidate.path)
    try {
      const existing = this.store.getSourceByPath(candidate.path)
      const stat = statSync(candidate.path)
      if (
        existing?.status === 'indexed' &&
        existing.byteSize === stat.size &&
        Math.round(existing.modifiedAt) === Math.round(stat.mtimeMs) &&
        existing.indexingProfile === profileKey
      ) {
        return true
      }
      const fingerprint = await fingerprintSource(candidate.path, signal)
      if (signal.aborted) return false
      this.store.markSourcePending({
        id: sourceId,
        rootId: candidate.root.id,
        path: candidate.path,
        fingerprint,
        mediaType: extname(candidate.path).slice(1).toLowerCase(),
        indexingProfile: profileKey,
      })
      const reusable = this.store.findReusableResult(fingerprint.contentHash, sourceId, profileKey)
      if (reusable) {
        this.store.saveResult(candidate.root.id, candidate.path, fingerprint, profileKey, reusable)
        return true
      }
      const request: IndexingRequest = {
        jobId,
        rootId: candidate.root.id,
        sourceId,
        path: candidate.path,
        fingerprint,
        depth: profile.depth,
        requestedCapabilities: profile.requestedCapabilities,
        maxPagesPerDocument: profile.maxPagesPerDocument,
        maxOcrPagesPerDocument: profile.maxOcrPagesPerDocument,
        ocrEveryPage: profile.ocrEveryPage,
      }
      const backend = this.backends.get(
        candidate.root.processingBackend === 'cloud' ? 'tezbar-cloud' : 'local'
      )
      if (!backend) throw new Error('Selected indexing backend is unavailable')
      const result = await backend.index(request, {
        signal,
        yieldToInteractiveWork: () => this.waitForInteractiveIdle(signal),
        onProgress: (fileProgress, detail) => {
          const overall = total > 0 ? (completed() + fileProgress) / total : 1
          this.updateStatus({
            ...this.status,
            progress: Math.max(this.status.progress, overall),
            detail: detail ? `${detail} · ${basename(candidate.path)}` : basename(candidate.path),
          })
        },
      })
      await this.waitForInteractiveIdle(signal)
      if (signal.aborted) return false
      this.store.saveResult(candidate.root.id, candidate.path, fingerprint, profileKey, result)
      return true
    } catch (error) {
      if (signal.aborted) return false
      this.store.markSourceFailed(sourceId, error instanceof Error ? error.message : String(error))
      return false
    }
  }

  private needsIndexing(root: KnowledgeRoot, path: string): boolean {
    const profile = profileForDepth(effectiveKnowledgeDepth(root, this.settings))
    if (!profile) return false
    try {
      const existing = this.store.getSourceByPath(path)
      const fileStat = statSync(path)
      return !(
        existing?.status === 'indexed' &&
        existing.byteSize === fileStat.size &&
        Math.round(existing.modifiedAt) === Math.round(fileStat.mtimeMs) &&
        existing.indexingProfile === indexingProfileKey(profile)
      )
    } catch {
      return false
    }
  }

  private activeRoots(): KnowledgeRoot[] {
    return this.store
      .listRoots()
      .filter((root) => root.enabled && effectiveKnowledgeDepth(root, this.settings) !== 'off')
  }

  private async restartForConfigurationChange(detail: string): Promise<KnowledgeSnapshot> {
    this.rescanRequested = false
    if (this.rescanTimer) clearTimeout(this.rescanTimer)
    this.rescanTimer = null
    if (this.mode === 'coordinator') {
      await this.workerHost?.stop()
    } else {
      const active = this.activePromise
      this.controller?.abort()
      const jobId = this.status.jobId
      if (jobId) await this.backends.get('local')?.cancel(jobId)
      if (active) await active.catch(() => {})
    }
    this.store.clearIndexingCheckpoint()

    if (this.manuallyPaused) {
      this.updateStatus({ ...this.status, state: 'paused', detail: 'Indexing paused' })
      return this.snapshot()
    }
    if (this.activeRoots().length === 0) {
      this.updateStatus({
        ...initialStatus(),
        state: 'idle',
        backend: 'local',
        progress: 1,
        detail: `${detail} · No active content-indexing folders`,
      })
      return this.snapshot()
    }
    return this.startIndexing()
  }

  private updateProgress(done: number, total: number, detail: string): void {
    this.updateStatus({
      ...this.status,
      progress: total > 0 ? Math.max(this.status.progress, Math.min(1, done / total)) : 1,
      queuedSources: Math.max(0, total - done),
      detail,
    })
  }

  private refreshCounts(): void {
    const counts = this.store.counts()
    this.status = { ...this.status, ...counts }
  }

  private updateStatus(status: KnowledgeStatus): void {
    const stateChanged = status.state !== this.status.state
    this.status = status
    this.refreshCounts()
    const now = Date.now()
    if (stateChanged || now - this.lastStatusPersistAt >= STATUS_PERSIST_INTERVAL_MS) {
      this.persistStatus()
      this.lastStatusPersistAt = now
    }
    if (stateChanged || now - this.lastStatusEventAt >= STATUS_EVENT_INTERVAL_MS) {
      this.statusSink(this.status)
      this.lastStatusEventAt = now
    }
  }

  private persistStatus(): void {
    this.store.saveStatus(this.status)
  }

  private async waitForInteractiveIdle(signal: AbortSignal): Promise<void> {
    while (!signal.aborted && Date.now() < this.interactiveUntil) {
      const remaining = this.interactiveUntil - Date.now()
      await new Promise<void>((resolve) => setTimeout(resolve, Math.min(100, remaining)))
    }
  }

  shutdown(): void {
    this.workerHost?.shutdown()
    this.controller?.abort()
    if (this.startupTimer) clearTimeout(this.startupTimer)
    this.startupTimer = null
    if (this.rescanTimer) clearTimeout(this.rescanTimer)
    this.rescanTimer = null
    for (const watcher of this.watchers.values()) watcher.close()
    this.watchers.clear()
    if (this.initialized) this.persistStatus()
  }

  private scheduleRescan(): void {
    if (this.manuallyPaused) {
      this.rescanRequested = true
      return
    }
    if (this.isIndexingActive()) {
      this.rescanRequested = true
      return
    }
    if (this.rescanTimer) clearTimeout(this.rescanTimer)
    this.rescanTimer = setTimeout(() => {
      this.rescanTimer = null
      void this.startIndexing()
    }, 1_500)
    this.rescanTimer.unref()
  }

  private syncWatchers(): void {
    if (this.mode === 'worker') return
    const enabledRoots = new Map(this.activeRoots().map((root) => [root.id, root]))
    for (const [rootId, watcher] of this.watchers) {
      if (enabledRoots.has(rootId)) continue
      watcher.close()
      this.watchers.delete(rootId)
    }
    for (const root of enabledRoots.values()) {
      if (this.watchers.has(root.id)) continue
      try {
        const watcher = watch(root.path, { recursive: true }, () => this.scheduleRescan())
        watcher.on('error', () => {
          watcher.close()
          this.watchers.delete(root.id)
        })
        this.watchers.set(root.id, watcher)
      } catch {
        // Manual reindex remains available when recursive watching is unsupported.
      }
    }
  }

  private isIndexingActive(): boolean {
    return this.activePromise !== null || Boolean(this.workerHost?.isRunning())
  }

  private async stopCoordinatorWorker(): Promise<void> {
    if (this.mode === 'coordinator') await this.workerHost?.stop()
  }

  private acceptWorkerStatus(status: KnowledgeStatus): void {
    this.status = { ...status }
    this.statusSink(this.status)
  }

  private handleWorkerExit(exit: {
    code: number | null
    signal: NodeJS.Signals | null
    expected: boolean
  }): void {
    if (!this.initialized || this.mode !== 'coordinator') return
    if (exit.expected) return
    const persisted = this.store.getPersistedStatus()
    this.status = persisted
    if (
      (this.status.state === 'scanning' || this.status.state === 'indexing')
    ) {
      this.updateStatus({
        ...this.status,
        state: 'failed',
        detail: 'Indexing worker stopped',
        error: `Indexing worker exited with ${exit.signal ?? exit.code ?? 'an unknown error'}`,
      })
    } else {
      this.statusSink(this.status)
    }
    if (this.rescanRequested && !this.manuallyPaused) {
      this.rescanRequested = false
      this.scheduleRescan()
    }
  }
}

let service: KnowledgeService | null = null

export function getKnowledgeService(): KnowledgeService {
  service ??= new KnowledgeService({ mode: 'coordinator' })
  service.initialize()
  return service
}

export async function chooseKnowledgeFolder(): Promise<string | null> {
  if (process.platform !== 'darwin') return null
  process.stdout.write(`${JSON.stringify({ type: 'window_suppress_blur', value: true })}\n`)
  try {
    const script =
      'POSIX path of (choose folder with prompt "Choose a folder for Tezbar Knowledge")'
    const { stdout } = await execFileAsync('/usr/bin/osascript', ['-e', script], {
      encoding: 'utf8',
    })
    const path = stdout.trim().replace(new RegExp(`${sep}$`), '')
    return path || null
  } catch {
    return null
  } finally {
    process.stdout.write(`${JSON.stringify({ type: 'window_suppress_blur', value: false })}\n`)
  }
}
