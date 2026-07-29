export const TERMINAL_IPC = {
  CREATE: 'terminal:create',
  ATTACH: 'terminal:attach',
  DETACH: 'terminal:detach',
  LIST: 'terminal:list',
  UPDATE: 'terminal:update',
  WRITE: 'terminal:write',
  RESIZE: 'terminal:resize',
  GET_CWD: 'terminal:get-cwd',
  KILL: 'terminal:kill',
  DELETE: 'terminal:delete',
  DATA: 'terminal:data',
  EXIT: 'terminal:exit',
  GET_PROMPT_INFO: 'terminal:get-prompt-info',
} as const

export function compactTerminalPath(value: string): string {
  const normalized = value.replace(/\\/g, '/')
  return normalized
    .replace(/^\/Users\/[^/]+(?=\/|$)/, '...')
    .replace(/^[A-Za-z]:\/Users\/[^/]+(?=\/|$)/, '...')
}

/**
 * Returns whether a path can be used as an explicit terminal working directory.
 * The launcher accepts `/path >`, `C:\\path >`, and UNC path prefixes.
 */
export function isAbsoluteTerminalPath(value: string): boolean {
  const path = value.trim()
  return path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path) || /^\\\\/.test(path)
}

export function terminalDirectoryLabel(cwd: string): string {
  const parts = cwd.replace(/\\/g, '/').split('/').filter(Boolean)
  const first = parts[0] ?? ''
  if (parts.length === 2 && parts[0] === 'Users') return '~'
  if (parts.length === 3 && parts[0] === 'Users') return `~/${parts[2]}`
  if (parts.length >= 4 && /^[A-Za-z]:$/.test(first) && parts[1] === 'Users') {
    return `~/${parts.slice(3).join('/')}`
  }
  if (parts.length === 0) return '/'
  return parts.slice(-2).join('/')
}

export function normalizeTerminalCommand(command: string | undefined): string {
  return command?.trim().replace(/\s+/g, ' ') ?? ''
}

export function formatTerminalSessionName(cwd: string, lastCommand: string): string {
  const directory = terminalDirectoryLabel(cwd)
  const command = normalizeTerminalCommand(lastCommand)
  return command ? `${directory} · ${command}` : directory
}

export type TerminalCreateRequest = {
  cwd?: string
  initialCommand?: string
  /** Reopen an exited session under the same id so its durable history can be replayed. */
  restoreSessionId?: string
  /** Legacy fallback shown without execution when a session predates durable history. */
  restoreCommand?: string
  name?: string
  saveFor?: TerminalSaveFor
  keepAliveFor?: TerminalKeepAliveFor
  cols: number
  rows: number
}

export type TerminalCreateResult = {
  sessionId: string
  shell: string
  cwd: string
  summary: TerminalSessionSummary
}

export type TerminalAttachRequest = {
  sessionId: string
  cols: number
  rows: number
}

export type TerminalAttachResult = {
  sessionId: string
  shell: string
  cwd: string
  recentOutput: string
  summary: TerminalSessionSummary
}

export type TerminalDataEvent = {
  sessionId: string
  data: string
}

export type TerminalExitEvent = {
  sessionId: string
  exitCode: number
  signal?: number
}

export type TerminalPromptInfo = {
  user: string
  host: string
  dir: string
}

export type TerminalSaveFor = 'day' | 'week' | 'month' | 'forever'

export type TerminalKeepAliveFor = '3h' | '8h' | 'day' | 'until-stop'

export type TerminalSessionStatus = 'running' | 'exited'

export type TerminalSessionSummary = {
  sessionId: string
  name: string
  cwd: string
  shell: string
  initialCommand?: string
  lastCommand?: string
  createdAt: number
  updatedAt: number
  lastActiveAt: number
  saveFor: TerminalSaveFor
  keepAliveFor: TerminalKeepAliveFor
  saveUntil?: number
  keepAliveUntil?: number
  status: TerminalSessionStatus
  exitCode?: number
  signal?: number
  pid?: number
}

export type TerminalUpdateRequest = {
  sessionId: string
  name?: string
  cwd?: string
  lastCommand?: string
  saveFor?: TerminalSaveFor
  keepAliveFor?: TerminalKeepAliveFor
}

export type TerminalSessionsAction =
  | { type: 'new' }
  | { type: 'select'; sessionId: string }
