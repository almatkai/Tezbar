import { app } from '@tezbar/desktop-runtime'
import { execFile, spawn, type ChildProcess } from 'node:child_process'
import { createHash, randomBytes } from 'node:crypto'
import { existsSync, createWriteStream, readdirSync, readFileSync } from 'node:fs'
import { chmod, mkdir, rename, rm, writeFile } from 'node:fs/promises'
import { availableParallelism } from 'node:os'
import { join } from 'node:path'
import { createServer } from 'node:net'
import { promisify } from 'node:util'
import { Readable } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import type { KnowledgeIndexingSpeed, SemanticSearchStatus } from '../../../shared/knowledge'
import {
  GEMMA_MODEL,
  GEMMA_MODEL_URL,
  gemmaRuntime,
  gemmaDocument,
  gemmaQuery,
  normalizeGemmaVector,
} from './gemmaManifest'

const execFileAsync = promisify(execFile)
const IDLE_MS = 120_000

function findServer(directory: string): string | undefined {
  if (!existsSync(directory)) return undefined
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (
      entry.isFile() &&
      entry.name === (process.platform === 'win32' ? 'llama-server.exe' : 'llama-server')
    )
      return path
    if (entry.isDirectory()) {
      const found = findServer(path)
      if (found) return found
    }
  }
  return undefined
}

async function freePort(): Promise<number> {
  const server = createServer()
  return new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      server.close((error) => {
        if (error || !address || typeof address === 'string')
          reject(error ?? new Error('Cannot allocate embedding port'))
        else resolve(address.port)
      })
    })
  })
}

/** Local inference and installation behind a batched, asynchronous interface. */
export class GemmaRuntime {
  private readonly directory = join(
    app.getPath('userData'),
    'knowledge',
    'models',
    'embeddinggemma-2'
  )
  private download: AbortController | null = null
  private downloadPromise: Promise<void> | null = null
  private child: ChildProcess | null = null
  private startup: Promise<void> | null = null
  private idleTimer: NodeJS.Timeout | null = null
  private endpoint = ''
  private token = ''
  private queue: Promise<unknown> = Promise.resolve()
  private error: string | undefined
  private downloadedBytes = 0
  private modelState: SemanticSearchStatus['modelState'] = 'not-downloaded'
  private closed = false
  private removing = false
  private stopping: Promise<void> = Promise.resolve()
  private speed: KnowledgeIndexingSpeed = 'medium'

  setSpeed(speed: KnowledgeIndexingSpeed): void {
    if (this.speed === speed) return
    const prevThreads = this.threadsForSpeed(this.speed)
    this.speed = speed
    const newThreads = this.threadsForSpeed(speed)
    if (this.child && prevThreads !== newThreads) {
      void this.stop()
    }
  }

  private threadsForSpeed(speed: KnowledgeIndexingSpeed): number {
    const maxAvailable = availableParallelism()
    if (speed === 'slow') return Math.min(2, Math.max(1, maxAvailable))
    if (speed === 'fast') return Math.min(8, Math.max(1, maxAvailable))
    return Math.min(4, Math.max(1, maxAvailable))
  }

  snapshot(): Pick<
    SemanticSearchStatus,
    | 'supported'
    | 'modelState'
    | 'downloadProgress'
    | 'downloadBytes'
    | 'downloadTotalBytes'
    | 'error'
  > {
    const runtime = gemmaRuntime()
    let installed = false
    try {
      const manifest = JSON.parse(readFileSync(join(this.directory, 'installed.json'), 'utf8')) as {
        revision: string
        runtime: string
      }
      installed =
        manifest.revision === GEMMA_MODEL.revision &&
        manifest.runtime === runtime?.file &&
        existsSync(join(this.directory, GEMMA_MODEL.file)) &&
        Boolean(findServer(join(this.directory, 'runtime')))
    } catch {
      /* No verified installation yet. */
    }
    const total = GEMMA_MODEL.bytes + (runtime?.bytes ?? 0)
    return {
      supported: Boolean(runtime),
      modelState: this.download ? 'downloading' : installed ? 'ready' : this.modelState,
      downloadProgress: installed ? 1 : Math.min(1, this.downloadedBytes / total),
      downloadBytes: installed ? total : this.downloadedBytes,
      downloadTotalBytes: total,
      error: this.error,
    }
  }

  startDownload(): void {
    if (this.closed) throw new Error('Embedding runtime is shutting down')
    if (this.removing) throw new Error('Embedding model is being removed')
    if (this.download || this.snapshot().modelState === 'ready') return
    const runtime = gemmaRuntime()
    if (!runtime) throw new Error('EmbeddingGemma is not available for this platform yet')
    this.download = new AbortController()
    this.downloadedBytes = 0
    this.error = undefined
    this.modelState = 'downloading'
    const signal = this.download.signal
    this.downloadPromise = (async () => {
      await mkdir(this.directory, { recursive: true })
      const runtimeDirectory = join(this.directory, 'runtime')
      const archive = join(this.directory, runtime.file)
      await this.downloadFile(runtime.url, archive, runtime.sha256, runtime.bytes, signal)
      await mkdir(runtimeDirectory, { recursive: true })
      if (runtime.file.endsWith('.zip')) {
        await execFileAsync(
          'powershell.exe',
          [
            '-NoLogo',
            '-NoProfile',
            '-NonInteractive',
            '-Command',
            'Expand-Archive -LiteralPath $env:TEZBAR_GEMMA_ARCHIVE -DestinationPath $env:TEZBAR_GEMMA_DESTINATION -Force',
          ],
          {
            windowsHide: true,
            signal,
            env: {
              ...process.env,
              TEZBAR_GEMMA_ARCHIVE: archive,
              TEZBAR_GEMMA_DESTINATION: runtimeDirectory,
            },
          }
        )
      } else {
        await execFileAsync('tar', ['-xzf', archive, '-C', runtimeDirectory], { signal })
      }
      const server = findServer(runtimeDirectory)
      if (!server) throw new Error('Downloaded runtime does not contain llama-server')
      if (process.platform !== 'win32') await chmod(server, 0o755)
      await rm(archive, { force: true })
      await this.downloadFile(
        GEMMA_MODEL_URL,
        join(this.directory, GEMMA_MODEL.file),
        GEMMA_MODEL.sha256,
        GEMMA_MODEL.bytes,
        signal
      )
      signal.throwIfAborted()
      await writeFile(
        join(this.directory, 'installed.json'),
        JSON.stringify({ revision: GEMMA_MODEL.revision, runtime: runtime.file })
      )
      this.modelState = 'ready'
    })()
      .catch((error: unknown) => {
        this.modelState = signal.aborted ? 'not-downloaded' : 'error'
        if (!signal.aborted) this.error = error instanceof Error ? error.message : String(error)
      })
      .finally(() => {
        this.download = null
        this.downloadPromise = null
      })
  }

  async cancelDownload(): Promise<void> {
    const wasDownloading = Boolean(this.download)
    this.download?.abort()
    await this.downloadPromise
    if (wasDownloading && this.modelState !== 'ready') {
      await rm(this.directory, { recursive: true, force: true })
      this.downloadedBytes = 0
    }
  }

  async remove(): Promise<void> {
    this.removing = true
    try {
      await this.cancelDownload()
      this.stop()
      await this.queue.catch(() => {})
      await this.stopping
      // This fixed, app-owned directory contains only this optional model.
      await rm(this.directory, { recursive: true, force: true })
      this.modelState = 'not-downloaded'
      this.downloadedBytes = 0
      this.error = undefined
    } finally {
      this.removing = false
    }
  }

  embedQuery(text: string, signal?: AbortSignal): Promise<number[]> {
    return this.embed([gemmaQuery(text)], signal).then((vectors) => vectors[0])
  }

  embedDocuments(
    documents: Array<{ text: string; title?: string }>,
    signal?: AbortSignal
  ): Promise<number[][]> {
    return this.embed(
      documents.map(({ text, title }) => gemmaDocument(text, title)),
      signal
    )
  }

  private embed(inputs: string[], signal?: AbortSignal): Promise<number[][]> {
    const run = this.queue
      .catch(() => {})
      .then(async () => {
        if (this.closed) throw new Error('Embedding runtime is shutting down')
        if (this.removing) throw new Error('Embedding model is being removed')
        signal?.throwIfAborted()
        if (this.idleTimer) clearTimeout(this.idleTimer)
        const abort = () => this.stop()
        signal?.addEventListener('abort', abort, { once: true })
        try {
          await this.ensureStarted(signal)
          signal?.throwIfAborted()
          const response = await fetch(`${this.endpoint}/v1/embeddings`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${this.token}` },
            body: JSON.stringify({ input: inputs, encoding_format: 'float' }),
            signal: signal
              ? AbortSignal.any([signal, AbortSignal.timeout(60_000)])
              : AbortSignal.timeout(60_000),
          })
          if (!response.ok) throw new Error(`Local embedding request failed (${response.status})`)
          const result = (await response.json()) as {
            data?: Array<{ index: number; embedding: unknown }>
          }
          const rows = result.data?.sort((left, right) => left.index - right.index)
          if (
            !rows ||
            rows.length !== inputs.length ||
            rows.some((row, index) => row.index !== index)
          ) {
            throw new Error('Local embedding response is incomplete')
          }
          this.error = undefined
          return rows.map((row) => normalizeGemmaVector(row.embedding))
        } catch (error) {
          if (!signal?.aborted) this.error = error instanceof Error ? error.message : String(error)
          throw error
        } finally {
          signal?.removeEventListener('abort', abort)
          this.idleTimer = setTimeout(() => this.stop(), IDLE_MS)
          this.idleTimer.unref()
        }
      })
    this.queue = run
    return run
  }

  private async ensureStarted(signal?: AbortSignal): Promise<void> {
    if (this.startup) return this.startup
    if (this.child) return
    this.startup = this.launch(signal).finally(() => {
      this.startup = null
    })
    return this.startup
  }

  private async launch(signal?: AbortSignal): Promise<void> {
    await this.stopping
    if (this.closed) throw new Error('Embedding runtime is shutting down')
    if (this.removing) throw new Error('Embedding model is being removed')
    if (this.snapshot().modelState !== 'ready')
      throw new Error('Download EmbeddingGemma before enabling it')
    const server = findServer(join(this.directory, 'runtime'))!
    const port = await freePort()
    signal?.throwIfAborted()
    if (this.removing || this.closed) throw new Error('Embedding runtime is stopping')
    this.token = randomBytes(32).toString('hex')
    this.endpoint = `http://127.0.0.1:${port}`
    const child = spawn(
      process.execPath,
      [
        join(__dirname, 'gemma-supervisor.cjs'),
        server,
        '--model',
        join(this.directory, GEMMA_MODEL.file),
        '--embedding',
        '--pooling',
        'mean',
        '--host',
        '127.0.0.1',
        '--port',
        String(port),
        '--api-key',
        this.token,
        '--ctx-size',
        '8192',
        '--batch-size',
        '8192',
        '--ubatch-size',
        '8192',
        '--parallel',
        '1',
        '--threads',
        String(this.threadsForSpeed(this.speed)),
      ],
      { windowsHide: true, stdio: ['pipe', 'ignore', 'pipe'] }
    )
    this.child = child
    child.stdin?.on('error', () => {})
    let failure: string | undefined
    let stderr = ''
    child.stderr?.on('data', (data: Buffer) => {
      stderr = (stderr + data.toString()).slice(-4000)
    })
    child.once('error', (error) => {
      failure = error.message
    })
    child.once('exit', (code) => {
      failure = `Local embedding runtime exited (${code}): ${stderr.slice(-700)}`
      if (this.child === child) this.child = null
    })
    try {
      const deadline = Date.now() + 60_000
      while (Date.now() < deadline) {
        signal?.throwIfAborted()
        if (failure || this.child !== child)
          throw new Error(failure ?? 'Local embedding runtime stopped')
        try {
          const response = await fetch(`${this.endpoint}/health`, {
            headers: { Authorization: `Bearer ${this.token}` },
            signal: AbortSignal.timeout(1000),
          })
          if (response.ok) return
        } catch {
          /* Server is still loading the model. */
        }
        await new Promise((resolve) => setTimeout(resolve, 200))
      }
      throw new Error('Local embedding runtime took too long to start')
    } catch (error) {
      this.stop()
      throw error
    }
  }

  stop(): void {
    if (this.idleTimer) clearTimeout(this.idleTimer)
    this.idleTimer = null
    const child = this.child
    this.child = null
    if (child) {
      this.stopping = new Promise((resolve) => {
        const force = setTimeout(() => {
          child.kill()
          resolve()
        }, 3000)
        force.unref()
        child.once('close', () => {
          clearTimeout(force)
          resolve()
        })
        // EOF lets the supervisor kill the native process before exiting.
        child.stdin?.end()
      })
    }
  }

  shutdown(): void {
    this.closed = true
    this.download?.abort()
    this.stop()
  }

  private async downloadFile(
    url: string,
    path: string,
    expectedHash: string,
    expectedBytes: number,
    signal: AbortSignal
  ): Promise<void> {
    const temporary = `${path}.part`
    try {
      const response = await fetch(url, {
        signal: AbortSignal.any([signal, AbortSignal.timeout(30 * 60_000)]),
      })
      if (!response.ok || !response.body)
        throw new Error(`Model download failed (${response.status})`)
      const hash = createHash('sha256')
      let size = 0
      const source = Readable.fromWeb(response.body as import('node:stream/web').ReadableStream)
      source.on('data', (chunk: Buffer) => {
        size += chunk.length
        this.downloadedBytes += chunk.length
        hash.update(chunk)
        if (size > expectedBytes)
          source.destroy(new Error('Downloaded file exceeds its expected size'))
      })
      await pipeline(source, createWriteStream(temporary), { signal })
      if (size !== expectedBytes || hash.digest('hex') !== expectedHash)
        throw new Error('Downloaded file failed checksum verification')
      signal.throwIfAborted()
      await rename(temporary, path)
    } finally {
      await rm(temporary, { force: true })
    }
  }
}
