import { setPriority } from 'node:os'
import type { KnowledgeStatus } from '../../shared/knowledge'
import { KnowledgeService } from './service'

try {
  // Keep the worker below interactive launcher/search work. Child OCR helpers
  // inherit this scheduling priority on macOS and Linux.
  setPriority(0, 10)
} catch {
  // Priority changes are best-effort on platforms that restrict setpriority.
}

function emitStatus(status: KnowledgeStatus): void {
  process.stdout.write(
    `${JSON.stringify({ type: 'event', channel: 'knowledge:status', payload: status })}\n`
  )
}

const service = new KnowledgeService({ mode: 'worker', statusSink: emitStatus })
let stopping = false

function shutdown(): void {
  if (stopping) return
  stopping = true
  service.shutdown()
  process.exit(0)
}

process.on('SIGINT', shutdown)
process.on('SIGTERM', shutdown)

// The supervisor may hard-kill a hung backend. Pipe EOF also reaches us in
// that case, so indexing cannot survive as an orphan alongside its replacement.
const bun = (
  globalThis as typeof globalThis & {
    Bun?: { stdin: { stream: () => ReadableStream<Uint8Array> } }
  }
).Bun
if (bun) {
  void (async () => {
    const reader = bun.stdin.stream().getReader()
    try {
      while (!(await reader.read()).done) {
        /* lifetime pipe carries no data */
      }
    } finally {
      reader.releaseLock()
      shutdown()
    }
  })().catch(shutdown)
} else {
  process.stdin.on('end', shutdown)
  process.stdin.on('error', shutdown)
  process.stdin.resume()
}

void (async () => {
  service.initialize()
  await service.startIndexing()
  await service.waitForCurrentRun()
  shutdown()
})().catch((error: unknown) => {
  console.error(error instanceof Error ? (error.stack ?? error.message) : String(error))
  service.shutdown()
  process.exit(1)
})
