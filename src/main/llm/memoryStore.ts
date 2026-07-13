import { app } from '@tezbar/desktop-runtime'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { redactSensitiveText } from '../safety/redaction'

type MemorySource = 'conversation' | 'clipboard' | 'manual'

type MemoryEntry = {
  id: string
  text: string
  source: MemorySource
  createdAt: number
  private?: boolean
}

type MemoryDb = {
  entries: MemoryEntry[]
}

export type MemoryRetrievalPolicy = {
  enabled: boolean
  maxItems: number
  includePrivate: boolean
}

function memoryPath(): string {
  const dir = join(app.getPath('userData'), 'llm')
  mkdirSync(dir, { recursive: true })
  return join(dir, 'memory.json')
}

function readDb(): MemoryDb {
  try {
    const raw = readFileSync(memoryPath(), 'utf8')
    const parsed = JSON.parse(raw) as Partial<MemoryDb>
    if (!Array.isArray(parsed.entries)) return { entries: [] }
    return { entries: parsed.entries }
  } catch {
    return { entries: [] }
  }
}

function writeDb(db: MemoryDb): void {
  writeFileSync(memoryPath(), `${JSON.stringify(db, null, 2)}\n`, 'utf8')
}

function tokenize(input: string): Set<string> {
  return new Set(
    input
      .toLowerCase()
      .split(/\s+/)
      .map((token) => token.replace(/[^a-z0-9_-]/g, ''))
      .filter((token) => token.length > 2),
  )
}

function overlapScore(query: Set<string>, text: Set<string>): number {
  if (query.size === 0 || text.size === 0) return 0
  let overlap = 0
  query.forEach((token) => {
    if (text.has(token)) overlap += 1
  })
  return overlap / query.size
}

export function rememberMemory(text: string, source: MemorySource, isPrivate = false): void {
  const cleaned = text.trim()
  if (!cleaned) return

  const db = readDb()
  db.entries = [
    {
      id: `mem:${Date.now()}:${Math.random().toString(36).slice(2, 8)}`,
      text: redactSensitiveText(cleaned),
      source,
      createdAt: Date.now(),
      private: isPrivate,
    },
    ...db.entries,
  ].slice(0, 500)
  writeDb(db)
}

export function retrieveMemories(query: string, policy: MemoryRetrievalPolicy): string[] {
  if (!policy.enabled || policy.maxItems <= 0) return []

  const queryTokens = tokenize(query)
  const db = readDb()

  return db.entries
    .filter((entry) => policy.includePrivate || !entry.private)
    .map((entry) => ({
      text: entry.text,
      score: overlapScore(queryTokens, tokenize(entry.text)),
      createdAt: entry.createdAt,
    }))
    .filter((entry) => entry.score > 0)
    .sort((a, b) => {
      if (b.score !== a.score) return b.score - a.score
      return b.createdAt - a.createdAt
    })
    .slice(0, policy.maxItems)
    .map((entry) => entry.text)
}
