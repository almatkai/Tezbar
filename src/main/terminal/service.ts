import type { WebContents } from 'electron'
import { existsSync, statSync } from 'node:fs'
import { homedir, hostname, userInfo } from 'node:os'
import { resolve } from 'node:path'
import { randomUUID } from 'node:crypto'
import type { IPty } from 'node-pty'
import { spawn } from 'node-pty'
import {
  TERMINAL_IPC,
  type TerminalCreateRequest,
  type TerminalCreateResult,
  type TerminalPromptInfo,
} from '../../shared/terminal'

type TerminalSession = {
  ownerId: number
  process: IPty
}

const sessions = new Map<string, TerminalSession>()
const ownerCleanupRegistered = new Set<number>()

function clampDimension(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min
  return Math.min(Math.max(Math.floor(value), min), max)
}

function resolveWorkingDirectory(raw?: string): string {
  const candidate = raw?.trim() ? resolve(raw) : homedir()
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
  env.TERM_PROGRAM = 'Raymes'
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

export function createTerminalSession(
  sender: WebContents,
  request: TerminalCreateRequest,
): TerminalCreateResult {
  const sessionId = randomUUID()
  const cwd = resolveWorkingDirectory(request.cwd)
  const shell = resolveShell()
  const cols = clampDimension(request.cols, 2, 500)
  const rows = clampDimension(request.rows, 2, 300)
  const args = process.platform === 'win32' ? [] : ['-l']
  const ptyProcess = spawn(shell, args, {
    name: 'xterm-256color',
    cols,
    rows,
    cwd,
    env: terminalEnvironment(),
  })

  sessions.set(sessionId, { ownerId: sender.id, process: ptyProcess })

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

  return { sessionId, shell, cwd }
}

export function writeTerminalSession(ownerId: number, sessionId: string, data: string): boolean {
  const session = sessionForOwner(sessionId, ownerId)
  if (!session || data.length === 0 || data.length > 64 * 1024) return false
  session.process.write(data)
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
