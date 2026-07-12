import { randomUUID } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import { app } from '@tezbar/desktop-runtime'
import type { NamedPortEntry } from '../../shared/portManager'

function storePath(): string {
  return `${app.getPath('userData')}/named-ports.json`
}

function readAll(): NamedPortEntry[] {
  const path = storePath()
  if (!existsSync(path)) return []
  try {
    const raw = readFileSync(path, 'utf8')
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    return parsed
      .map((row) => {
        if (!row || typeof row !== 'object') return null
        const o = row as Record<string, unknown>
        const id = typeof o.id === 'string' ? o.id : ''
        const name = typeof o.name === 'string' ? o.name.trim() : ''
        const port = typeof o.port === 'number' ? o.port : Number(o.port)
        if (!id || !name || !Number.isFinite(port) || port < 1 || port > 65535) return null
        return { id, name, port: Math.floor(port) } satisfies NamedPortEntry
      })
      .filter((x): x is NamedPortEntry => x !== null)
  } catch {
    return []
  }
}

function writeAll(entries: NamedPortEntry[]): void {
  const path = storePath()
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, `${JSON.stringify(entries, null, 2)}\n`, 'utf8')
}

export function listNamedPorts(): NamedPortEntry[] {
  return readAll().sort((a, b) => a.name.localeCompare(b.name) || a.port - b.port)
}

export function addNamedPort(name: string, port: number): NamedPortEntry | null {
  const trimmed = name.trim()
  if (!trimmed || port < 1 || port > 65535) return null
  const entries = readAll()
  const next: NamedPortEntry = { id: randomUUID(), name: trimmed, port }
  entries.push(next)
  writeAll(entries)
  return next
}

export function removeNamedPort(id: string): boolean {
  const entries = readAll()
  const next = entries.filter((e) => e.id !== id)
  if (next.length === entries.length) return false
  writeAll(next)
  return true
}
