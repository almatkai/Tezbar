import type { WebContents } from 'electron'
import { existsSync, statSync } from 'node:fs'
import { homedir, hostname, userInfo } from 'node:os'
import { join, resolve } from 'node:path'
import { randomUUID } from 'node:crypto'
import { spawn as spawnChild } from 'node:child_process'
import { createRequire } from 'node:module'
import type { IPty } from 'node-pty'
import {
  TERMINAL_IPC,
  type TerminalCreateRequest,
  type TerminalCreateResult,
  type TerminalPromptInfo,
} from '../../shared/terminal'

const requireNative = createRequire(__filename)

type TerminalSession = {
  ownerId: number
  sender: WebContents
  process: IPty
  pipeMode: boolean
}

type BunPipeProcess = {
  pid: number
  stdin: { write(data: string): number; flush?: () => number | Promise<number> }
  stdout: ReadableStream<Uint8Array>
  stderr: ReadableStream<Uint8Array>
  exited: Promise<number>
  kill(): void
}

type BunRuntime = {
  spawn(
    command: string[],
    options: {
      cwd: string
      env: Record<string, string>
      stdin: 'pipe'
      stdout: 'pipe'
      stderr: 'pipe'
    },
  ): BunPipeProcess
}

function spawnBunPipeTerminal(
  shell: string,
  args: string[],
  cwd: string,
  env: Record<string, string>,
  cols: number,
  rows: number,
): IPty {
  const bun = (globalThis as typeof globalThis & { Bun?: BunRuntime }).Bun
  if (!bun) return spawnPipeTerminal(shell, args, cwd, env, cols, rows)

  const child = bun.spawn([shell, ...args], {
    cwd,
    env,
    stdin: 'pipe',
    stdout: 'pipe',
    stderr: 'pipe',
  })
  const dataListeners = new Set<(data: string) => void>()

  const pump = async (stream: ReadableStream<Uint8Array>): Promise<void> => {
    const reader = stream.getReader()
    const decoder = new TextDecoder()
    try {
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        const text = decoder.decode(value, { stream: true })
        if (text) dataListeners.forEach((listener) => listener(text))
      }
    } catch {
      // Stream closure races with process termination.
    } finally {
      reader.releaseLock()
    }
  }
  void pump(child.stdout)
  void pump(child.stderr)

  return {
    pid: child.pid,
    process: shell,
    cols,
    rows,
    handleFlowControl: false,
    onData: (listener: (data: string) => void) => {
      dataListeners.add(listener)
      return { dispose: () => dataListeners.delete(listener) }
    },
    onExit: (listener: (event: { exitCode: number; signal?: number }) => void) => {
      let active = true
      void child.exited.then((exitCode) => {
        if (active) listener({ exitCode, signal: 0 })
      })
      return { dispose: () => { active = false } }
    },
    write: (data: string) => {
      child.stdin.write(data)
      void child.stdin.flush?.()
    },
    resize: () => undefined,
    clear: () => undefined,
    pause: () => undefined,
    resume: () => undefined,
    kill: () => child.kill(),
  } as IPty
}

function spawnPipeTerminal(
  shell: string,
  args: string[],
  cwd: string,
  env: Record<string, string>,
  cols: number,
  rows: number,
): IPty {
  const child = spawnChild(shell, args, { cwd, env, stdio: ['pipe', 'pipe', 'pipe'] })
  return {
    pid: child.pid ?? -1,
    process: shell,
    cols,
    rows,
    handleFlowControl: false,
    onData: (listener: (data: string) => void) => {
      const onData = (chunk: Buffer): void => listener(chunk.toString('utf8'))
      child.stdout.on('data', onData)
      child.stderr.on('data', onData)
      return { dispose: () => {
        child.stdout.off('data', onData)
        child.stderr.off('data', onData)
      } }
    },
    onExit: (listener: (event: { exitCode: number; signal?: number }) => void) => {
      const onExit = (exitCode: number | null, signal: NodeJS.Signals | null): void => {
        listener({ exitCode: exitCode ?? 1, signal: signal ? 1 : 0 })
      }
      child.once('exit', onExit)
      return { dispose: () => child.off('exit', onExit) }
    },
    write: (data: string) => { child.stdin.write(data) },
    resize: () => undefined,
    clear: () => undefined,
    pause: () => child.stdout.pause(),
    resume: () => child.stdout.resume(),
    kill: () => { child.kill() },
  } as IPty
}

const sessions = new Map<string, TerminalSession>()
const ownerCleanupRegistered = new Set<number>()

function clampDimension(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min
  return Math.min(Math.max(Math.floor(value), min), max)
}

function resolveWorkingDirectory(raw?: string): string {
  const requested = raw?.trim()
  const expanded = requested === '~'
    ? homedir()
    : requested?.startsWith('~/')
      ? join(homedir(), requested.slice(2))
      : requested
  const candidate = expanded ? resolve(expanded) : homedir()
  try {
    return existsSync(candidate) && statSync(candidate).isDirectory() ? candidate : homedir()
  } catch {
    return homedir()
  }
}

function resolveShell(): string {
  const configured = process.env.SHELL?.trim()
  if (configured && configured.startsWith('/') && existsSync(configured)) return configured
  return process.platform === 'win32' ? 'powershell.exe' : '/bin/zsh'
}

function terminalEnvironment(): Record<string, string> {
  const env: Record<string, string> = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (typeof value === 'string') env[key] = value
  }
  env.TERM = 'xterm-256color'
  env.COLORTERM = 'truecolor'
  env.TERM_PROGRAM = 'Tezbar'
  return env
}

function killOwnerSessions(ownerId: number): void {
  for (const [sessionId, session] of sessions) {
    if (session.ownerId !== ownerId) continue
    sessions.delete(sessionId)
    try {
      session.process.kill()
    } catch {
      // The shell may already have exited.
    }
  }
}

function sessionForOwner(sessionId: string, ownerId: number): TerminalSession | null {
  const session = sessions.get(sessionId)
  return session?.ownerId === ownerId ? session : null
}

function pipeInputEcho(data: string): string {
  if (data === '\x7f') return '\b \b'
  if (data.startsWith('\x1b')) return ''
  return data.replace(/\r/g, '\r\n').replace(/[^\x20-\x7e\r\n\b\t]/g, '')
}

export function createTerminalSession(
  sender: WebContents,
  request: TerminalCreateRequest,
): TerminalCreateResult {
  const sessionId = randomUUID()
  const cwd = resolveWorkingDirectory(request.cwd)
  const shell = resolveShell()
  const cols = clampDimension(request.cols, 2, 500)
  const rows = clampDimension(request.rows, 2, 300)
  // Interactive zsh reopens /dev/tty under Bun and bypasses the JSONL pipes.
  // Electron uses node-pty below, while the Tauri sidecar stays pipe-safe.
  const args = process.platform === 'win32' ? [] : ['-l']
  const env = terminalEnvironment()
  const ptyProcess = process.versions.bun
    ? spawnBunPipeTerminal(shell, args, cwd, env, cols, rows)
    : (requireNative('node-pty') as typeof import('node-pty')).spawn(shell, args, {
      name: 'xterm-256color',
      cols,
      rows,
      cwd,
      env,
    })

  sessions.set(sessionId, {
    ownerId: sender.id,
    sender,
    process: ptyProcess,
    pipeMode: Boolean(process.versions.bun),
  })

  if (!ownerCleanupRegistered.has(sender.id)) {
    ownerCleanupRegistered.add(sender.id)
    sender.once('destroyed', () => {
      ownerCleanupRegistered.delete(sender.id)
      killOwnerSessions(sender.id)
    })
  }

  ptyProcess.onData((data) => {
    if (!sender.isDestroyed()) {
      sender.send(TERMINAL_IPC.DATA, { sessionId, data })
    }
  })

  ptyProcess.onExit(({ exitCode, signal }) => {
    sessions.delete(sessionId)
    if (!sender.isDestroyed()) {
      sender.send(TERMINAL_IPC.EXIT, { sessionId, exitCode, signal })
    }
  })

  if (request.initialCommand) {
    ptyProcess.write(`${request.initialCommand}${process.versions.bun ? '\n' : '\r'}`)
  }

  return { sessionId, shell, cwd }
}

export function writeTerminalSession(ownerId: number, sessionId: string, data: string): boolean {
  const session = sessionForOwner(sessionId, ownerId)
  if (!session || data.length === 0 || data.length > 64 * 1024) return false
  if (session.pipeMode) {
    const echo = pipeInputEcho(data)
    if (echo && !session.sender.isDestroyed()) {
      session.sender.send(TERMINAL_IPC.DATA, { sessionId, data: echo })
    }
  }
  session.process.write(session.pipeMode ? data.replace(/\r/g, '\n') : data)
  return true
}

export function resizeTerminalSession(
  ownerId: number,
  sessionId: string,
  cols: number,
  rows: number,
): boolean {
  const session = sessionForOwner(sessionId, ownerId)
  if (!session) return false
  session.process.resize(clampDimension(cols, 2, 500), clampDimension(rows, 2, 300))
  return true
}

export function killTerminalSession(ownerId: number, sessionId: string): boolean {
  const session = sessionForOwner(sessionId, ownerId)
  if (!session) return false
  sessions.delete(sessionId)
  try {
    session.process.kill()
  } catch {
    // The shell may already have exited.
  }
  return true
}

export function getTerminalPromptInfo(): TerminalPromptInfo {
  const user = userInfo().username
  const host = hostname().split('.')[0]
  const dir = '~'
  return { user, host, dir }
}

export function shutdownTerminalSessions(): void {
  for (const session of sessions.values()) {
    try {
      session.process.kill()
    } catch {
      // Best-effort shutdown during application quit.
    }
  }
  sessions.clear()
}
