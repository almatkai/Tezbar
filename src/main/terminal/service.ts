import type { WebContents } from '@tezbar/desktop-runtime'
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readlinkSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { homedir, hostname, userInfo } from 'node:os'
import { join, resolve } from 'node:path'
import { randomUUID } from 'node:crypto'
import { execFileSync, spawn as spawnChild } from 'node:child_process'
import {
  TERMINAL_IPC,
  formatTerminalSessionName,
  type TerminalAttachRequest,
  type TerminalAttachResult,
  type TerminalCreateRequest,
  type TerminalCreateResult,
  type TerminalKeepAliveFor,
  type TerminalPromptInfo,
  type TerminalSaveFor,
  type TerminalSessionSummary,
  type TerminalUpdateRequest,
} from '../../shared/terminal'
import { readRawConfig, writeConfigPatch } from '../llm/configStore'

type TerminalProcess = {
  pid: number
  process: string
  cols: number
  rows: number
  handleFlowControl: boolean
  onData: (listener: (data: string) => void) => { dispose: () => void }
  onExit: (listener: (event: { exitCode: number; signal?: number }) => void) => {
    dispose: () => void
  }
  write: (data: string) => void
  resize: (cols: number, rows: number) => void
  clear: () => void
  pause: () => void
  resume: () => void
  kill: () => void
}

type TerminalSession = {
  ownerId: number
  attachedSenders: Map<number, WebContents>
  process: TerminalProcess
  pipeMode: boolean
  shell: string
  cwd: string
  historyPath: string
  summary: TerminalSessionSummary
  outputChunks: string[]
  outputBytes: number
  backgroundTimer: ReturnType<typeof setTimeout> | null
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
): TerminalProcess {
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
  }
}

function spawnPipeTerminal(
  shell: string,
  args: string[],
  cwd: string,
  env: Record<string, string>,
  cols: number,
  rows: number,
): TerminalProcess {
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
  }
}

const sessions = new Map<string, TerminalSession>()
const ownerCleanupRegistered = new Set<number>()
const persistedSummaries = new Map<string, TerminalSessionSummary>()
let persistedLoaded = false
const OUTPUT_REPLAY_LIMIT_BYTES = 512 * 1024
const TERMINAL_CONFIG_KEY = 'terminalSessions'
const TERMINAL_HISTORY_DIR = join(homedir(), '.openray', 'terminal-history')

const SAVE_FOR_MS: Record<Exclude<TerminalSaveFor, 'forever'>, number> = {
  day: 24 * 60 * 60 * 1000,
  week: 7 * 24 * 60 * 60 * 1000,
  month: 30 * 24 * 60 * 60 * 1000,
}

const KEEP_ALIVE_MS: Record<Exclude<TerminalKeepAliveFor, 'until-stop'>, number> = {
  '3h': 3 * 60 * 60 * 1000,
  '8h': 8 * 60 * 60 * 1000,
  day: 24 * 60 * 60 * 1000,
}

function clampDimension(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min
  return Math.min(Math.max(Math.floor(value), min), max)
}

function normalizeTerminalPath(raw: string): string {
  const requested = raw.trim().replace(/[\\/]\.\.\.$/, '')
  if (!requested) return ''

  if (requested === '~') return homedir()
  if (requested.startsWith('~/')) return join(homedir(), requested.slice(2))

  // The launcher uses `/Desktop`, `/Documents`, etc. as a compact cross-
  // platform spelling. On Windows those are not drive-root folders; map them
  // to the current user's profile before resolving the terminal cwd.
  if (process.platform === 'win32' && /^\/(Desktop|Documents|Downloads|Pictures|Videos|Music)(?:\/|$)/i.test(requested)) {
    return join(homedir(), requested.slice(1))
  }

  return requested
}

function resolveWorkingDirectory(raw?: string): string {
  const requested = raw?.trim()
  const expanded = requested ? normalizeTerminalPath(requested) : undefined
  const candidate = expanded ? resolve(expanded) : homedir()
  try {
    return existsSync(candidate) && statSync(candidate).isDirectory() ? candidate : homedir()
  } catch {
    return homedir()
  }
}

function resolveExistingWorkingDirectory(raw: string): string | null {
  const requested = normalizeTerminalPath(raw)
  if (!requested) return null
  const candidate = resolve(requested)
  try {
    return existsSync(candidate) && statSync(candidate).isDirectory() ? candidate : null
  } catch {
    return null
  }
}

function processWorkingDirectory(pid: number): string | null {
  try {
    if (process.platform === 'darwin') {
      const output = execFileSync(
        '/usr/sbin/lsof',
        ['-a', '-p', String(pid), '-d', 'cwd', '-Fn'],
        { encoding: 'utf8', timeout: 1_000 },
      )
      const path = output
        .split(/\r?\n/)
        .find((line) => line.startsWith('n'))
        ?.slice(1)
      return path ? resolveExistingWorkingDirectory(path) : null
    }
    if (process.platform === 'linux') {
      return resolveExistingWorkingDirectory(readlinkSync(`/proc/${pid}/cwd`))
    }
  } catch {
    // The process may exit between looking it up and reading its cwd.
  }
  return null
}

function normalizeLastCommand(raw: string): string | undefined {
  const command = raw.trim().replace(/\s+/g, ' ')
  return command ? command.slice(0, 4_096) : undefined
}

function validHistorySessionId(sessionId: string): boolean {
  return /^[a-zA-Z0-9_-]{1,200}$/.test(sessionId)
}

function terminalHistoryPath(sessionId: string): string {
  if (!validHistorySessionId(sessionId)) throw new Error('Invalid terminal session id')
  mkdirSync(TERMINAL_HISTORY_DIR, { recursive: true, mode: 0o700 })
  return join(TERMINAL_HISTORY_DIR, `${sessionId}.log`)
}

function readTerminalHistory(session: TerminalSession): string {
  try {
    return readFileSync(session.historyPath, 'utf8')
  } catch {
    return session.outputChunks.join('')
  }
}

function removeTerminalHistory(sessionId: string): void {
  if (!validHistorySessionId(sessionId)) return
  try {
    rmSync(terminalHistoryPath(sessionId), { force: true })
  } catch {
    // History cleanup is best effort; session metadata remains authoritative.
  }
}

function legacyTerminalHistory(command: string | undefined): string {
  const safeCommand = command?.replace(/[\x00-\x1f\x7f]/g, ' ').trim().slice(0, 4_096)
  if (!safeCommand) return ''
  return `[Previous output was not recorded by this app version]\n$ ${safeCommand}\n\n`
}

function isSaveFor(value: unknown): value is TerminalSaveFor {
  return value === 'day' || value === 'week' || value === 'month' || value === 'forever'
}

function isKeepAliveFor(value: unknown): value is TerminalKeepAliveFor {
  return value === '3h' || value === '8h' || value === 'day' || value === 'until-stop'
}

function saveUntil(saveFor: TerminalSaveFor, from: number): number | undefined {
  return saveFor === 'forever' ? undefined : from + SAVE_FOR_MS[saveFor]
}

function keepAliveUntil(keepAliveFor: TerminalKeepAliveFor, from: number): number | undefined {
  return keepAliveFor === 'until-stop' ? undefined : from + KEEP_ALIVE_MS[keepAliveFor]
}

function defaultSessionName(cwd: string, initialCommand?: string): string {
  return formatTerminalSessionName(cwd, initialCommand?.split('\n')[0] ?? '')
}

function normalizeSessionName(raw: unknown, cwd: string, initialCommand?: string): string {
  if (typeof raw === 'string') {
    const trimmed = raw.trim().replace(/\s+/g, ' ')
    if (trimmed) return trimmed.slice(0, 120)
  }
  return defaultSessionName(cwd, initialCommand)
}

function isTerminalSummary(value: unknown): value is TerminalSessionSummary {
  if (!value || typeof value !== 'object') return false
  const row = value as Partial<TerminalSessionSummary>
  return (
    typeof row.sessionId === 'string' &&
    typeof row.name === 'string' &&
    typeof row.cwd === 'string' &&
    typeof row.shell === 'string' &&
    typeof row.createdAt === 'number' &&
    typeof row.updatedAt === 'number' &&
    typeof row.lastActiveAt === 'number' &&
    isSaveFor(row.saveFor) &&
    isKeepAliveFor(row.keepAliveFor) &&
    (row.status === 'running' || row.status === 'exited')
  )
}

function loadPersistedSummaries(): void {
  if (persistedLoaded) return
  persistedLoaded = true
  const raw = readRawConfig()[TERMINAL_CONFIG_KEY]
  if (!Array.isArray(raw)) return
  for (const item of raw) {
    if (!isTerminalSummary(item)) continue
    persistedSummaries.set(item.sessionId, {
      ...item,
      status: sessions.has(item.sessionId) ? item.status : 'exited',
      pid: sessions.has(item.sessionId) ? item.pid : undefined,
      keepAliveUntil: sessions.has(item.sessionId) ? item.keepAliveUntil : undefined,
    })
  }
}

function writePersistedSummaries(): void {
  loadPersistedSummaries()
  writeConfigPatch({ [TERMINAL_CONFIG_KEY]: Array.from(persistedSummaries.values()) })
}

function prunePersistedSummaries(now = Date.now()): void {
  loadPersistedSummaries()
  let changed = false
  for (const [sessionId, summary] of persistedSummaries) {
    if (sessions.has(sessionId)) continue
    if (summary.saveUntil !== undefined && summary.saveUntil <= now) {
      persistedSummaries.delete(sessionId)
      removeTerminalHistory(sessionId)
      changed = true
    }
  }
  if (changed) writePersistedSummaries()
}

function persistSessionSummary(summary: TerminalSessionSummary): void {
  loadPersistedSummaries()
  persistedSummaries.set(summary.sessionId, { ...summary })
  writePersistedSummaries()
}

function appendOutput(session: TerminalSession, data: string): void {
  try {
    appendFileSync(session.historyPath, data, 'utf8')
  } catch {
    // Keep the live terminal functional if durable history is temporarily unavailable.
  }
  session.outputChunks.push(data)
  session.outputBytes += Buffer.byteLength(data, 'utf8')
  while (session.outputBytes > OUTPUT_REPLAY_LIMIT_BYTES && session.outputChunks.length > 1) {
    const removed = session.outputChunks.shift() ?? ''
    session.outputBytes -= Buffer.byteLength(removed, 'utf8')
  }
}

function sendToAttached(session: TerminalSession, channel: string, payload: unknown): void {
  for (const [senderId, sender] of session.attachedSenders) {
    if (sender.isDestroyed()) {
      session.attachedSenders.delete(senderId)
      continue
    }
    sender.send(channel, payload)
  }
}

function clearBackgroundTimer(session: TerminalSession): void {
  if (!session.backgroundTimer) return
  clearTimeout(session.backgroundTimer)
  session.backgroundTimer = null
}

function markSessionActive(session: TerminalSession, now = Date.now()): void {
  session.summary.updatedAt = now
  session.summary.lastActiveAt = now
  session.summary.status = 'running'
  session.summary.exitCode = undefined
  session.summary.signal = undefined
}

function markSessionExited(
  session: TerminalSession,
  exitCode: number,
  signal?: number,
  now = Date.now(),
): void {
  clearBackgroundTimer(session)
  sessions.delete(session.summary.sessionId)
  session.summary.status = 'exited'
  session.summary.exitCode = exitCode
  session.summary.signal = signal
  session.summary.pid = undefined
  session.summary.updatedAt = now
  session.summary.lastActiveAt = now
  session.summary.keepAliveUntil = undefined
  session.summary.saveUntil = saveUntil(session.summary.saveFor, now)
  persistSessionSummary(session.summary)
  sendToAttached(session, TERMINAL_IPC.EXIT, {
    sessionId: session.summary.sessionId,
    exitCode,
    signal,
  })
}

function scheduleBackgroundKill(session: TerminalSession): void {
  clearBackgroundTimer(session)
  if (session.attachedSenders.size > 0 || session.summary.status !== 'running') return
  const now = Date.now()
  session.summary.keepAliveUntil = keepAliveUntil(session.summary.keepAliveFor, now)
  persistSessionSummary(session.summary)
  if (session.summary.keepAliveUntil === undefined) return
  const delay = Math.max(0, session.summary.keepAliveUntil - now)
  session.backgroundTimer = setTimeout(() => {
    if (session.attachedSenders.size > 0 || session.summary.status !== 'running') return
    try {
      session.process.kill()
    } catch {
      markSessionExited(session, 1)
    }
  }, delay)
}

function registerOwnerCleanup(sender: WebContents): void {
  if (ownerCleanupRegistered.has(sender.id)) return
  ownerCleanupRegistered.add(sender.id)
  sender.once('destroyed', () => {
    ownerCleanupRegistered.delete(sender.id)
    detachOwnerSessions(sender.id)
  })
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

function detachOwnerSessions(ownerId: number): void {
  for (const session of sessions.values()) {
    if (session.ownerId !== ownerId) continue
    session.attachedSenders.delete(ownerId)
    scheduleBackgroundKill(session)
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
  loadPersistedSummaries()
  if (request.restoreSessionId && !validHistorySessionId(request.restoreSessionId)) {
    throw new Error('Invalid terminal session id')
  }
  const restoredSummary = request.restoreSessionId
    ? persistedSummaries.get(request.restoreSessionId)
    : undefined
  const sessionId = restoredSummary ? restoredSummary.sessionId : randomUUID()
  if (sessions.has(sessionId)) throw new Error('Terminal session is already running')
  const historyPath = terminalHistoryPath(sessionId)
  if (!restoredSummary) writeFileSync(historyPath, '', { encoding: 'utf8', mode: 0o600 })
  else if (!existsSync(historyPath) || statSync(historyPath).size === 0) {
    writeFileSync(historyPath, legacyTerminalHistory(request.restoreCommand), {
      encoding: 'utf8',
      mode: 0o600,
    })
  }
  const cwd = resolveWorkingDirectory(request.cwd)
  const shell = resolveShell()
  const cols = clampDimension(request.cols, 2, 500)
  const rows = clampDimension(request.rows, 2, 300)
  const now = Date.now()
  const saveFor = isSaveFor(request.saveFor) ? request.saveFor : 'week'
  const keepAliveFor = isKeepAliveFor(request.keepAliveFor) ? request.keepAliveFor : '3h'
  // Interactive zsh reopens /dev/tty under Bun and bypasses the JSONL pipes,
  // so the Tauri sidecar uses a login shell over explicit pipes.
  const args = process.platform === 'win32' ? [] : ['-l']
  const env = terminalEnvironment()
  const ptyProcess = spawnBunPipeTerminal(shell, args, cwd, env, cols, rows)

  const summary: TerminalSessionSummary = restoredSummary
    ? {
        ...restoredSummary,
        cwd,
        shell,
        updatedAt: now,
        lastActiveAt: now,
        saveUntil: saveUntil(restoredSummary.saveFor, now),
        keepAliveUntil: undefined,
        status: 'running',
        exitCode: undefined,
        signal: undefined,
        pid: ptyProcess.pid,
      }
    : {
        sessionId,
        name: normalizeSessionName(request.name, cwd, request.initialCommand),
        cwd,
        shell,
        initialCommand: request.initialCommand?.trim() || undefined,
        lastCommand: request.initialCommand
          ? normalizeLastCommand(request.initialCommand)
          : undefined,
        createdAt: now,
        updatedAt: now,
        lastActiveAt: now,
        saveFor,
        keepAliveFor,
        saveUntil: saveUntil(saveFor, now),
        status: 'running',
        pid: ptyProcess.pid,
      }
  const session: TerminalSession = {
    ownerId: sender.id,
    attachedSenders: new Map([[sender.id, sender]]),
    process: ptyProcess,
    pipeMode: Boolean(process.versions.bun),
    shell,
    cwd,
    historyPath,
    summary,
    outputChunks: [],
    outputBytes: 0,
    backgroundTimer: null,
  }
  sessions.set(sessionId, session)
  persistSessionSummary(summary)
  registerOwnerCleanup(sender)

  const initialCommand = request.initialCommand
  let initialCommandWritten = false
  const writeInitialCommand = (): void => {
    if (!initialCommand || initialCommandWritten) return
    initialCommandWritten = true
    ptyProcess.write(`${initialCommand}${process.versions.bun ? '\n' : '\r'}`)
  }

  ptyProcess.onData((data) => {
    const current = sessions.get(sessionId)
    if (!current) return
    markSessionActive(current)
    appendOutput(current, data)
    sendToAttached(current, TERMINAL_IPC.DATA, { sessionId, data })
    if (!process.versions.bun) writeInitialCommand()
  })

  ptyProcess.onExit(({ exitCode, signal }) => {
    const current = sessions.get(sessionId)
    if (!current) return
    markSessionExited(current, exitCode, signal)
  })

  if (process.versions.bun) writeInitialCommand()

  return { sessionId, shell, cwd, summary }
}

export function attachTerminalSession(
  sender: WebContents,
  request: TerminalAttachRequest,
): TerminalAttachResult | null {
  const session = sessionForOwner(request.sessionId, sender.id)
  if (!session || session.summary.status !== 'running') return null
  registerOwnerCleanup(sender)
  session.attachedSenders.set(sender.id, sender)
  clearBackgroundTimer(session)
  session.summary.keepAliveUntil = undefined
  session.process.resize(clampDimension(request.cols, 2, 500), clampDimension(request.rows, 2, 300))
  markSessionActive(session)
  persistSessionSummary(session.summary)
  return {
    sessionId: request.sessionId,
    shell: session.shell,
    cwd: session.cwd,
    recentOutput: readTerminalHistory(session),
    summary: { ...session.summary },
  }
}

export function detachTerminalSession(ownerId: number, sessionId: string): boolean {
  const session = sessionForOwner(sessionId, ownerId)
  if (!session) return false
  session.attachedSenders.delete(ownerId)
  scheduleBackgroundKill(session)
  return true
}

export function listTerminalSessions(ownerId: number): TerminalSessionSummary[] {
  prunePersistedSummaries()
  const byId = new Map<string, TerminalSessionSummary>()
  for (const summary of persistedSummaries.values()) {
    byId.set(summary.sessionId, { ...summary })
  }
  for (const session of sessions.values()) {
    if (session.ownerId !== ownerId) continue
    byId.set(session.summary.sessionId, { ...session.summary })
  }
  return Array.from(byId.values()).sort((a, b) => {
    if (a.status !== b.status) return a.status === 'running' ? -1 : 1
    return b.updatedAt - a.updatedAt
  })
}

export function updateTerminalSession(
  ownerId: number,
  request: TerminalUpdateRequest,
): TerminalSessionSummary | null {
  loadPersistedSummaries()
  if (!request.sessionId) return null
  const session = sessionForOwner(request.sessionId, ownerId)
  const existing = session?.summary ?? persistedSummaries.get(request.sessionId)
  if (!existing) return null
  const now = Date.now()
  if (request.name !== undefined) {
    existing.name = normalizeSessionName(request.name, existing.cwd, existing.initialCommand)
  }
  if (request.cwd !== undefined) {
    const cwd = resolveExistingWorkingDirectory(request.cwd)
    if (cwd) {
      existing.cwd = cwd
      if (session) session.cwd = cwd
    }
  }
  if (request.lastCommand !== undefined) {
    existing.lastCommand = normalizeLastCommand(request.lastCommand)
  }
  if (request.saveFor !== undefined && isSaveFor(request.saveFor)) {
    existing.saveFor = request.saveFor
    existing.saveUntil = saveUntil(request.saveFor, now)
  }
  if (request.keepAliveFor !== undefined && isKeepAliveFor(request.keepAliveFor)) {
    existing.keepAliveFor = request.keepAliveFor
    if (session && session.attachedSenders.size === 0) {
      existing.keepAliveUntil = keepAliveUntil(request.keepAliveFor, now)
    }
  }
  existing.updatedAt = now
  existing.lastActiveAt = Math.max(existing.lastActiveAt, now)
  persistSessionSummary(existing)
  if (session) scheduleBackgroundKill(session)
  return { ...existing }
}

export function recordNativeTerminalSession(request: {
  sessionId: string
  shell: string
  cwd: string
  initialCommand?: string
  name?: string
  saveFor?: TerminalSaveFor
  keepAliveFor?: TerminalKeepAliveFor
}): TerminalSessionSummary {
  loadPersistedSummaries()
  const now = Date.now()
  const cwd = resolveWorkingDirectory(request.cwd)
  const saveFor = isSaveFor(request.saveFor) ? request.saveFor : 'week'
  const keepAliveFor = isKeepAliveFor(request.keepAliveFor) ? request.keepAliveFor : '3h'
  const initialCommand = request.initialCommand?.trim() || undefined
  const summary: TerminalSessionSummary = {
    sessionId: request.sessionId,
    name: normalizeSessionName(request.name, cwd, initialCommand),
    cwd,
    shell: request.shell,
    initialCommand,
    lastCommand: initialCommand ? normalizeLastCommand(initialCommand) : undefined,
    createdAt: now,
    updatedAt: now,
    lastActiveAt: now,
    saveFor,
    keepAliveFor,
    saveUntil: saveUntil(saveFor, now),
    keepAliveUntil: undefined,
    status: 'running',
    pid: undefined,
  }
  persistSessionSummary(summary)
  return { ...summary }
}

export function markNativeTerminalSessionAttached(
  sessionId: string,
): TerminalSessionSummary | null {
  loadPersistedSummaries()
  const summary = persistedSummaries.get(sessionId)
  if (!summary) return null
  const now = Date.now()
  summary.updatedAt = now
  summary.lastActiveAt = now
  summary.status = 'running'
  summary.exitCode = undefined
  summary.signal = undefined
  summary.keepAliveUntil = undefined
  persistSessionSummary(summary)
  return { ...summary }
}

export function markNativeTerminalSessionRestored(request: {
  sessionId: string
  shell: string
  cwd: string
}): TerminalSessionSummary | null {
  loadPersistedSummaries()
  const summary = persistedSummaries.get(request.sessionId)
  if (!summary) return null
  const now = Date.now()
  summary.shell = request.shell
  summary.cwd = resolveExistingWorkingDirectory(request.cwd) ?? summary.cwd
  summary.updatedAt = now
  summary.lastActiveAt = now
  summary.status = 'running'
  summary.exitCode = undefined
  summary.signal = undefined
  summary.pid = undefined
  summary.keepAliveUntil = undefined
  summary.saveUntil = saveUntil(summary.saveFor, now)
  persistSessionSummary(summary)
  return { ...summary }
}

export function markNativeTerminalSessionExited(
  sessionId: string,
  exitCode: number,
  signal?: number,
): TerminalSessionSummary | null {
  loadPersistedSummaries()
  const summary = persistedSummaries.get(sessionId)
  if (!summary) return null
  const now = Date.now()
  summary.updatedAt = now
  summary.lastActiveAt = now
  summary.status = 'exited'
  summary.exitCode = exitCode
  summary.signal = signal
  summary.pid = undefined
  summary.keepAliveUntil = undefined
  summary.saveUntil = saveUntil(summary.saveFor, now)
  persistSessionSummary(summary)
  return { ...summary }
}

export function writeTerminalSession(ownerId: number, sessionId: string, data: string): boolean {
  const session = sessionForOwner(sessionId, ownerId)
  if (!session || data.length === 0 || data.length > 64 * 1024) return false
  if (session.pipeMode) {
    const echo = pipeInputEcho(data)
    if (echo) {
      appendOutput(session, echo)
      sendToAttached(session, TERMINAL_IPC.DATA, { sessionId, data: echo })
    }
  }
  markSessionActive(session)
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

export function getTerminalSessionCwd(ownerId: number, sessionId: string): string | null {
  const session = sessionForOwner(sessionId, ownerId)
  if (!session) return null
  const cwd = processWorkingDirectory(session.process.pid) ?? session.cwd
  session.cwd = cwd
  return cwd
}

export function killTerminalSession(ownerId: number, sessionId: string): boolean {
  const session = sessionForOwner(sessionId, ownerId)
  if (!session) {
    loadPersistedSummaries()
    const deleted = persistedSummaries.delete(sessionId)
    if (deleted) {
      removeTerminalHistory(sessionId)
      writePersistedSummaries()
    }
    return deleted
  }
  try {
    session.process.kill()
  } catch {
    markSessionExited(session, 1)
  }
  return true
}

export function deleteTerminalSession(ownerId: number, sessionId: string): boolean {
  const session = sessionForOwner(sessionId, ownerId)
  if (session) return false
  loadPersistedSummaries()
  const deleted = persistedSummaries.delete(sessionId)
  if (deleted) {
    removeTerminalHistory(sessionId)
    writePersistedSummaries()
  }
  return deleted
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
    session.summary.status = 'exited'
    session.summary.pid = undefined
    session.summary.keepAliveUntil = undefined
    session.summary.updatedAt = Date.now()
    session.summary.lastActiveAt = session.summary.updatedAt
    session.summary.saveUntil = saveUntil(session.summary.saveFor, session.summary.updatedAt)
    persistSessionSummary(session.summary)
  }
  sessions.clear()
}
