// src/main/server.ts
import { registerIpcHandlers, shutdownIpcHandlers } from './ipc'
import { startClipboardWatcher, stopClipboardWatcher } from './search/providers/clipboardProvider'
import { flushConfig, writeConfigPatch } from './llm/configStore'
import { BrowserWindow, ipcMain } from 'electron'
import readline from 'node:readline'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'

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

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false
})

rl.on('line', async (line) => {
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
      try {
        const args = Array.isArray(payload) ? payload : [payload]
        const result = await tauriIpcMain._invoke(channel, ...args)
        console.log(JSON.stringify({ type: 'reply', id, result }))
      } catch (error: unknown) {
        console.log(JSON.stringify({
          type: 'reply',
          id,
          error: error instanceof Error ? error.message : String(error),
        }))
      }
    }
  } catch (error: unknown) {
    console.error('[server] error parsing/handling stdin line:', error)
  }
})

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
rl.on('close', cleanup)

console.error('[server] Raymes TS background runner started successfully via stdin/stdout IPC.')
