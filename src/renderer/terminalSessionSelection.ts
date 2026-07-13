import type { TerminalSessionSummary } from '../shared/terminal'

type TerminalSessionShortcutEvent = {
  altKey: boolean
  code: string
  ctrlKey: boolean
  key: string
  metaKey: boolean
  shiftKey: boolean
}

export function terminalSessionShortcutIndex(
  event: TerminalSessionShortcutEvent
): number | null {
  if ((!event.metaKey && !event.ctrlKey) || event.altKey || event.shiftKey) return null

  const codeDigit = /^(?:Digit|Numpad)([1-9])$/.exec(event.code)?.[1]
  const digit = codeDigit ?? (/^[1-9]$/.test(event.key) ? event.key : null)
  return digit ? Number(digit) - 1 : null
}

export function terminalSessionAtIndex<T>(
  sessions: readonly T[],
  selectedIndex: number
): T | undefined {
  if (selectedIndex < 0) return undefined
  return sessions[selectedIndex]
}

export function terminalSessionRestoreOptions(session: TerminalSessionSummary): {
  restoreSessionId: string
  restoreCommand?: string
  workingDirectory: string
  name: string
  saveFor: TerminalSessionSummary['saveFor']
  keepAliveFor: TerminalSessionSummary['keepAliveFor']
} {
  return {
    restoreSessionId: session.sessionId,
    restoreCommand: session.lastCommand ?? session.initialCommand,
    workingDirectory: session.cwd,
    name: session.name,
    saveFor: session.saveFor,
    keepAliveFor: session.keepAliveFor,
  }
}

export function moveTerminalSelectionDown(currentIndex: number, sessionCount: number): number {
  if (sessionCount <= 0) return -1
  if (currentIndex < 0 || currentIndex >= sessionCount) return 0
  return Math.min(currentIndex + 1, sessionCount - 1)
}
