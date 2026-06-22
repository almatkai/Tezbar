// src/main/server.ts
import { registerIpcHandlers, shutdownIpcHandlers } from './ipc'
import { startClipboardWatcher, stopClipboardWatcher } from './search/providers/clipboardProvider'
import { flushConfig, writeConfigPatch } from './llm/configStore'
import { BrowserWindow, ipcMain } from 'electron'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'
import { createConnection } from 'node:net'

declare const __RAYMES_PI_POLICY_SOURCE__: string

function materializePiPolicy(): void {
  const root = process.env.APPDATA_DIR
  if (!root || typeof __RAYMES_PI_POLICY_SOURCE__ !== 'string') return
  try {
    const runtimeDir = join(root, 'runtime')
    const extensionPath = join(runtimeDir, 'raymes-pi-policy.ts')
    mkdirSync(runtimeDir, { recursive: true })
    writeFileSync(extensionPath, __RAYMES_PI_POLICY_SOURCE__, 'utf8')
    process.env.RAYMES_PI_EXTENSION = extensionPath
  } catch (error) {
    console.error('[server] failed to materialize Pi policy:', error)
  }
}

// Fix PATH for spawned child processes/extensions running inside Bun
function fixPathSync(): void {
  if (process.platform === 'win32') return
  try {
    const stdout = execFileSync('bash', ['-lc', 'echo -n $PATH'], {
      encoding: 'utf8',
      timeout: 2000,
    })
    const fromShell = stdout.trim()
    if (fromShell) {
      process.env.PATH = fromShell
    }
  } catch (err) {
    console.warn('[server] failed to get PATH from login shell:', err)
  }

  const extras = ['/opt/homebrew/bin', '/usr/local/bin', '/usr/bin', '/bin']
  const existing = new Set((process.env.PATH || '').split(':').filter(Boolean))
  const currentPaths = (process.env.PATH || '').split(':')
  for (const e of extras) {
    if (!existing.has(e)) {
      currentPaths.push(e)
    }
  }
  process.env.PATH = currentPaths.filter(Boolean).join(':')
}

fixPathSync()
materializePiPolicy()

const mockWin = new BrowserWindow()
const tauriIpcMain = ipcMain as typeof ipcMain & {
  _invoke: (channel: string, ...args: unknown[]) => Promise<unknown>
}

registerIpcHandlers(() => mockWin, {
  startWindowDragMonitoring: () => {},
  stopWindowDragMonitoring: () => {},
  updateRaymesHotkey: (h: string) => {
    writeConfigPatch({ raymesHotkey: h })
    return { ok: true, accelerator: h }
  }
})

startClipboardWatcher()

function writeReply(payload: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(payload)}\n`)
}

async function handleLine(line: string): Promise<void> {
  if (!line.trim()) return
  try {
    const message = JSON.parse(line) as {
      type?: string
      id?: unknown
      channel?: unknown
      payload?: unknown
    }
    if (message.type === 'invoke') {
      const { id, channel, payload } = message
      if ((typeof id !== 'string' && typeof id !== 'number') || typeof channel !== 'string') return
      const startedAt = Date.now()
      try {
        const args = Array.isArray(payload) ? payload : [payload]
        const result = await tauriIpcMain._invoke(channel, ...args)
        const elapsedMs = Date.now() - startedAt
        if (elapsedMs >= 1_000) {
          console.warn(`[server] slow IPC: ${channel} completed in ${elapsedMs}ms`)
        }
        writeReply({ type: 'reply', id, result })
      } catch (error: unknown) {
        console.error(`[server] IPC failed: ${channel}`, error)
        writeReply({
          type: 'reply',
          id,
          error: error instanceof Error ? error.message : String(error),
        })
      }
    }
  } catch (error: unknown) {
    console.error('[server] error parsing/handling stdin line:', error)
  }
}

let stdinBuffer = ''
function processInputChunk(chunk: string): void {
  stdinBuffer += chunk
  let newlineIndex = stdinBuffer.indexOf('\n')
  while (newlineIndex >= 0) {
    const line = stdinBuffer.slice(0, newlineIndex)
    stdinBuffer = stdinBuffer.slice(newlineIndex + 1)
    void handleLine(line)
    newlineIndex = stdinBuffer.indexOf('\n')
  }
}

type BunStdinRuntime = {
  stdin: { stream: () => ReadableStream<Uint8Array> }
}

const bunRuntime = (globalThis as typeof globalThis & { Bun?: BunStdinRuntime }).Bun
const backendIpcPort = Number(process.env.BACKEND_IPC_PORT)
if (Number.isInteger(backendIpcPort) && backendIpcPort > 0 && backendIpcPort <= 65_535) {
  const socket = createConnection({ host: '127.0.0.1', port: backendIpcPort }, () => {
    console.error(`[server] Connected to Tauri IPC on localhost:${backendIpcPort}`)
  })
  socket.setEncoding('utf8')
  socket.on('data', processInputChunk)
  socket.on('end', cleanup)
  socket.on('error', (error) => {
    console.error('[server] Tauri IPC socket failed:', error)
    cleanup()
  })
} else if (bunRuntime) {
  void (async () => {
    const reader = bunRuntime.stdin.stream().getReader()
    const decoder = new TextDecoder()
    try {
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        processInputChunk(decoder.decode(value, { stream: true }))
      }
      processInputChunk(decoder.decode())
    } finally {
      reader.releaseLock()
    }
    cleanup()
  })().catch((error: unknown) => {
    console.error('[server] native Bun stdin reader failed:', error)
    cleanup()
  })
} else {
  process.stdin.setEncoding('utf8')
  process.stdin.on('data', processInputChunk)
  process.stdin.on('end', cleanup)
  process.stdin.resume()
}

function cleanup(): void {
  try {
    stopClipboardWatcher()
    shutdownIpcHandlers()
    flushConfig()
  } catch (err) {
    console.error('[server] error during cleanup:', err)
  }
  process.exit(0)
}

process.on('SIGINT', cleanup)
process.on('SIGTERM', cleanup)

console.error('[server] Raymes TS background runner started successfully via stdin/stdout IPC.')
