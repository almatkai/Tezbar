import type { KnowledgeIndexingSpeed, KnowledgeSearchHit, SemanticSearchStatus } from '../../../shared/knowledge'
import { SPEED_PROFILES } from '../depth'
import type { KnowledgeStore } from '../database/store'
import { GemmaRuntime } from './gemmaRuntime'
import { fuseKnowledgeHits } from './gemmaIndex'

export type SemanticSearchOptions = {
  enabled: () => boolean
  roots: () => string[]
  allRoots: () => string[]
  canIndex: () => boolean
  waitForIdle: (signal: AbortSignal) => Promise<void>
  speed?: () => KnowledgeIndexingSpeed
  onProgress?: (force?: boolean) => void
}

/** Coordinates query inference and resumable embedding of already extracted text. */
export class SemanticSearch {
  private timer: NodeJS.Timeout | null = null
  private controller: AbortController | null = null
  private run: Promise<void> | null = null
  private error: string | undefined
  private closed = false
  private rescheduleRequested = false
  private readonly queryCache = new Map<string, number[]>()
  private countsCache: {
    roots: string
    at: number
    indexedChunks: number
    totalChunks: number
  } | null = null

  constructor(
    private readonly store: KnowledgeStore,
    private readonly options: SemanticSearchOptions,
    private readonly runtime = new GemmaRuntime()
  ) {}

  snapshot(): SemanticSearchStatus {
    const runtime = this.runtime.snapshot()
    const counts =
      runtime.modelState === 'ready' && this.options.enabled()
        ? this.counts()
        : { indexedChunks: 0, totalChunks: 0 }
    return {
      ...runtime,
      ...counts,
      indexing: this.controller !== null,
      error: this.error ?? runtime.error,
    }
  }

  private counts() {
    const rootIds = this.options.roots()
    const roots = rootIds.join('\n')
    // Background-task snapshots must not scan a large corpus on every tick.
    if (
      !this.countsCache ||
      this.countsCache.roots !== roots ||
      Date.now() - this.countsCache.at > 10_000
    ) {
      const counts = this.store.getGemmaIndex().counts(rootIds)
      // Stamp completion: a slow first count must not arrive already expired.
      this.countsCache = { roots, ...counts, at: Date.now() }
    }
    return this.countsCache
  }

  setSpeed(speed: KnowledgeIndexingSpeed): void {
    this.runtime.setSpeed(speed)
  }

  download(): void {
    this.runtime.startDownload()
    this.options.onProgress?.(true)
  }
  cancelDownload(): Promise<void> {
    const cancel = this.runtime.cancelDownload()
    this.options.onProgress?.(true)
    return cancel
  }

  async remove(): Promise<void> {
    await this.pause()
    await this.runtime.remove()
    this.store.getGemmaIndex().clear()
    this.queryCache.clear()
    this.countsCache = null
    this.error = undefined
    this.options.onProgress?.(true)
  }

  schedule(delay = 1000): void {
    if (this.closed || !this.options.enabled() || !this.options.canIndex()) return
    if (this.run) {
      this.rescheduleRequested = true
      return
    }
    if (this.timer) return
    this.timer = setTimeout(() => {
      this.timer = null
      if (
        !this.options.enabled() ||
        !this.options.canIndex() ||
        this.runtime.snapshot().modelState !== 'ready'
      )
        return
      this.controller = new AbortController()
      const signal = this.controller.signal
      this.options.onProgress?.(true)
      this.run = this.backfill(signal)
        .catch((error: unknown) => {
          if (!signal.aborted) this.error = error instanceof Error ? error.message : String(error)
        })
        .finally(() => {
          this.run = null
          this.controller = null
          this.countsCache = null
          this.options.onProgress?.(true)
          if (this.rescheduleRequested) {
            this.rescheduleRequested = false
            this.schedule()
          }
        })
    }, delay)
    this.timer.unref()
  }

  private async backfill(signal: AbortSignal): Promise<void> {
    this.error = undefined
    let afterRowId = 0
    while (!signal.aborted && this.options.enabled() && this.options.canIndex()) {
      await this.options.waitForIdle(signal)
      signal.throwIfAborted()
      const rootIds = this.options.roots()
      const speed = this.options.speed?.() ?? 'medium'
      const profile = SPEED_PROFILES[speed]
      const chunks = this.store.getGemmaIndex().pending(rootIds, profile.gemmaBatchSize, afterRowId)
      if (!chunks.length) return
      const vectors = await this.runtime.embedDocuments(chunks, signal)
      signal.throwIfAborted()
      const saved = this.store.getGemmaIndex().save(chunks, vectors)
      afterRowId = chunks[chunks.length - 1].rowId
      if (this.countsCache?.roots === rootIds.join('\n')) {
        this.countsCache.indexedChunks = Math.min(
          this.countsCache.totalChunks,
          this.countsCache.indexedChunks + saved
        )
      }
      this.options.onProgress?.(false)
      if (profile.gemmaDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, profile.gemmaDelayMs))
      } else {
        await new Promise<void>((resolve) => setImmediate(resolve))
      }
    }
  }

  async search(query: string, limit: number, allRootIds: string[]): Promise<KnowledgeSearchHit[]> {
    if (!query.trim() || limit <= 0) return []
    const lexical = this.store.search(query, Math.max(40, limit * 4), allRootIds, true)
    if (!this.options.enabled()) return lexical.slice(0, limit)
    const rootIds = this.options.roots()
    if (!rootIds.length || !this.store.getGemmaIndex().hasEmbeddings(rootIds))
      return lexical.slice(0, limit)
    try {
      const trimmed = query.trim()
      let vector = this.queryCache.get(trimmed)
      if (!vector) {
        vector = await this.runtime.embedQuery(trimmed)
        if (this.queryCache.size >= 100)
          this.queryCache.delete(this.queryCache.keys().next().value!)
        this.queryCache.set(trimmed, vector)
      }
      // Folder/model selection may have changed while inference was running.
      const currentLexical = this.store.search(
        query,
        Math.max(40, limit * 4),
        this.options.allRoots(),
        true
      )
      if (!this.options.enabled()) return currentLexical.slice(0, limit)
      const currentRoots = this.options.roots().filter((id) => allRootIds.includes(id))
      const semantic = this.store
        .getGemmaIndex()
        .search(vector, currentRoots, Math.max(40, limit * 4))
      this.error = undefined
      return fuseKnowledgeHits(currentLexical, semantic, limit)
    } catch (error) {
      this.error = error instanceof Error ? error.message : String(error)
      return this.store.search(query, limit, this.options.allRoots(), true)
    }
  }

  async pause(): Promise<void> {
    if (this.timer) clearTimeout(this.timer)
    this.timer = null
    this.rescheduleRequested = false
    this.controller?.abort()
    await this.run
    this.options.onProgress?.(true)
  }

  shutdown(): void {
    this.closed = true
    if (this.timer) clearTimeout(this.timer)
    this.timer = null
    this.controller?.abort()
    this.runtime.shutdown()
  }
}
