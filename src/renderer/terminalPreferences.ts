import type { TerminalKeepAliveFor, TerminalSaveFor } from '../shared/terminal'

export type TerminalDefaults = {
  saveFor: TerminalSaveFor
  keepAliveFor: TerminalKeepAliveFor
}

export const DEFAULT_TERMINAL_DEFAULTS: TerminalDefaults = {
  saveFor: 'week',
  keepAliveFor: '3h',
}

const STORAGE_KEY = 'tezbar:terminal-defaults'

function isSaveFor(value: unknown): value is TerminalSaveFor {
  return value === 'day' || value === 'week' || value === 'month' || value === 'forever'
}

function isKeepAliveFor(value: unknown): value is TerminalKeepAliveFor {
  return value === '3h' || value === '8h' || value === 'day' || value === 'until-stop'
}

export function readTerminalDefaults(): TerminalDefaults {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null') as Partial<TerminalDefaults> | null
    return {
      saveFor: isSaveFor(raw?.saveFor) ? raw.saveFor : DEFAULT_TERMINAL_DEFAULTS.saveFor,
      keepAliveFor: isKeepAliveFor(raw?.keepAliveFor)
        ? raw.keepAliveFor
        : DEFAULT_TERMINAL_DEFAULTS.keepAliveFor,
    }
  } catch {
    return { ...DEFAULT_TERMINAL_DEFAULTS }
  }
}

export function writeTerminalDefaults(value: TerminalDefaults): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value))
  } catch {
    // Storage can be unavailable in private/webview contexts.
  }
}
