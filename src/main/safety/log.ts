import { app } from '@tezbar/desktop-runtime'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { SafetyLogEntry } from '../../shared/safety'

const MAX_ENTRIES = 200

let cache: SafetyLogEntry[] | null = null

function logPath(): string {
  const dir = join(app.getPath('userData'), 'safety')
  mkdirSync(dir, { recursive: true })
  return join(dir, 'action-log.json')
}

function load(): SafetyLogEntry[] {
  if (cache) return cache
  try {
    const raw = readFileSync(logPath(), 'utf8')
    const parsed = JSON.parse(raw) as { entries?: unknown }
    cache = Array.isArray(parsed.entries) ? (parsed.entries as SafetyLogEntry[]) : []
  } catch {
    cache = []
  }
  return cache
}

function persist(): void {
  if (!cache) return
  try {
    writeFileSync(logPath(), JSON.stringify({ entries: cache }, null, 2), 'utf8')
  } catch {
    // Non-fatal: keep the in-memory log even if disk write fails.
  }
}

function truncateContext(
  context: Record<string, unknown> | undefined,
): Record<string, unknown> | undefined {
  if (!context) return undefined
  const out: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(context)) {
    if (typeof value === 'string') {
      out[key] = value.length > 200 ? `${value.slice(0, 200)}…` : value
    } else if (value === null || ['number', 'boolean'].includes(typeof value)) {
      out[key] = value
    } else {
      try {
        const serialised = JSON.stringify(value)
        out[key] = serialised.length > 200 ? `${serialised.slice(0, 200)}…` : serialised
      } catch {
        out[key] = '[unserializable]'
      }
    }
  }
  return out
}

export function recordSafetyEntry(entry: Omit<SafetyLogEntry, 'id' | 'at'>): SafetyLogEntry {
  const full: SafetyLogEntry = {
    ...entry,
    id: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
    at: Date.now(),
    context: truncateContext(entry.context),
  }
  const list = load()
  list.unshift(full)
  if (list.length > MAX_ENTRIES) list.length = MAX_ENTRIES
  persist()
  return full
}

export function listSafetyLog(): SafetyLogEntry[] {
  return load().slice()
}

export function clearSafetyLog(): void {
  cache = []
  persist()
}
