import { app } from '@tezbar/desktop-runtime'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { QuickNoteEntry } from '../../../shared/quickNotes'
import type { IndexedDocument, SearchProvider } from './types'

const NOTES_LIMIT = 250

type NotesDb = {
  notes: QuickNoteEntry[]
}

function stripMarkdownSyntax(text: string): string {
  return text
    .replace(/\*\*([^*\n]+)\*\*/g, '$1')
    .replace(/__([^_\n]+)__/g, '$1')
    .replace(/\*([^*\n]+)\*/g, '$1')
    .replace(/_([^_\n]+)_/g, '$1')
    .replace(/`([^`\n]+)`/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/^#{1,6}\s+/gm, '')
    .replace(/^\s*[-*+]\s+/gm, '')
    .replace(/^\s*\d+\.\s+/gm, '')
}

function decodeBasicEntities(text: string): string {
  return text
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&quot;/gi, '"')
    .replace(/&#039;/gi, "'")
}

function notePlainText(text: string): string {
  const withoutHtml = text
    .replace(/<br\s*\/?\s*>/gi, '\n')
    .replace(/<\/(div|p|li|h[1-6])>/gi, '\n')
    .replace(/<li>/gi, '- ')
    .replace(/<[^>]+>/g, '')

  return stripMarkdownSyntax(decodeBasicEntities(withoutHtml))
    .replace(/\r/g, '')
    .replace(/\u00a0/g, ' ')
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

function notesPath(): string {
  const dir = join(app.getPath('userData'), 'search')
  mkdirSync(dir, { recursive: true })
  return join(dir, 'notes.json')
}

function migrateNote(raw: unknown): QuickNoteEntry | null {
  if (!raw || typeof raw !== 'object') return null
  const o = raw as Record<string, unknown>
  if (typeof o.text !== 'string' || typeof o.createdAt !== 'number') return null
  const updatedAt = typeof o.updatedAt === 'number' ? o.updatedAt : o.createdAt
  return {
    text: o.text,
    createdAt: o.createdAt,
    updatedAt,
  }
}

function readNotesDb(): NotesDb {
  try {
    const raw = readFileSync(notesPath(), 'utf8')
    const parsed = JSON.parse(raw) as Partial<NotesDb>
    const notes: QuickNoteEntry[] = []
    if (Array.isArray(parsed.notes)) {
      for (const row of parsed.notes) {
        const m = migrateNote(row)
        if (m) notes.push(m)
      }
    }
    return { notes }
  } catch {
    return { notes: [] }
  }
}

function writeNotesDb(db: NotesDb): void {
  writeFileSync(notesPath(), `${JSON.stringify(db, null, 2)}\n`, 'utf8')
}

export function listQuickNotes(): QuickNoteEntry[] {
  return readNotesDb().notes
}

export function addQuickNote(text: string): QuickNoteEntry | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  const now = Date.now()
  const entry: QuickNoteEntry = { text: trimmed, createdAt: now, updatedAt: now }
  const db = readNotesDb()
  db.notes = [entry, ...db.notes].slice(0, NOTES_LIMIT)
  writeNotesDb(db)
  return entry
}

export function updateQuickNote(createdAt: number, text: string): boolean {
  const db = readNotesDb()
  const idx = db.notes.findIndex((n) => n.createdAt === createdAt)
  if (idx < 0) return false
  db.notes[idx] = {
    ...db.notes[idx]!,
    text,
    updatedAt: Date.now(),
  }
  writeNotesDb(db)
  return true
}

export function deleteQuickNote(createdAt: number): boolean {
  const db = readNotesDb()
  const next = db.notes.filter((n) => n.createdAt !== createdAt)
  if (next.length === db.notes.length) return false
  db.notes = next
  writeNotesDb(db)
  return true
}

/** @deprecated use listQuickNotes */
export function readQuickNotes(): QuickNoteEntry[] {
  return listQuickNotes()
}

export const notesProvider: SearchProvider = {
  providerId: 'notes',
  async buildDocuments(): Promise<IndexedDocument[]> {
    return listQuickNotes().map((note) => {
      const plain = notePlainText(note.text)
      return {
        id: `note:${note.createdAt}`,
        category: 'quick-notes',
        title: plain.split('\n')[0]?.trim().slice(0, 100) || '(note)',
        subtitle: 'Quick note',
        tokens: plain,
        action: { type: 'copy-text', text: plain },
        updatedAt: note.updatedAt,
      }
    })
  },
}
