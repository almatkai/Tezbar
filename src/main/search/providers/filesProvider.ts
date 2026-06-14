import { execFile } from 'node:child_process'
import { existsSync, readdirSync, statSync, watch } from 'node:fs'
import { homedir } from 'node:os'
import { extname, join, sep } from 'node:path'
import { promisify } from 'node:util'
import type { SearchResult } from '../../../shared/search'
import type { IndexedDocument } from './types'

const execFileAsync = promisify(execFile)

const ALLOWED_EXTENSIONS = new Set([
  '',
  '.md',
  '.txt',
  '.json',
  '.yaml',
  '.yml',
  '.ts',
  '.tsx',
  '.js',
  '.jsx',
  '.go',
  '.py',
  '.rs',
  '.swift',
  '.pdf',
  '.png',
  '.jpg',
])

const SKIP_NAMES = new Set([
  'node_modules',
  '.git',
  '.next',
  '.cache',
  'Library',
  'build',
  'coverage',
  'dist',
  'out',
  'target',
])

type FileChangeBatch = {
  upserts: IndexedDocument[]
  removeIds: string[]
}

type FileChangeListener = (payload: FileChangeBatch) => void

function isAllowedFile(path: string): boolean {
  const ext = extname(path).toLowerCase()
  return ALLOWED_EXTENSIONS.has(ext)
}

function containsSkippedDirectory(path: string): boolean {
  return path.split(sep).some((part) => SKIP_NAMES.has(part))
}

function makeFileDocument(path: string): IndexedDocument | null {
  try {
    const stat = statSync(path)
    if (!stat.isFile()) return null
    if (!isAllowedFile(path)) return null

    const title = path.split('/').pop() ?? path
    return {
      id: `file:${path}`,
      category: 'files',
      title,
      subtitle: path,
      tokens: `${title} ${path}`,
      action: { type: 'open-file', path },
      updatedAt: stat.mtimeMs,
      sourcePath: path,
      sourceMtime: stat.mtimeMs,
    }
  } catch {
    return null
  }
}

function initialRoots(): string[] {
  const home = homedir()
  return [join(home, 'Desktop'), join(home, 'Documents'), join(home, 'Downloads')].filter((root) => existsSync(root))
}

export async function collectInitialFileDocuments(limit = 4000): Promise<IndexedDocument[]> {
  const roots = initialRoots()
  if (roots.length === 0) return []

  const out: IndexedDocument[] = []
  const queue = [...roots]
  let visitedEntries = 0

  while (queue.length > 0 && out.length < limit) {
    const current = queue.shift()
    if (!current) continue
    try {
      const entries = readdirSync(current, { withFileTypes: true })
      for (const entry of entries) {
        if (out.length >= limit) break
        visitedEntries += 1
        if (visitedEntries % 250 === 0) {
          await new Promise<void>((resolve) => setImmediate(resolve))
        }
        const absolute = join(current, entry.name)
        if (entry.isDirectory()) {
          if (!SKIP_NAMES.has(entry.name)) queue.push(absolute)
          continue
        }
        const doc = makeFileDocument(absolute)
        if (doc) out.push(doc)
      }
    } catch {
      // Ignore unreadable directories.
    }
  }

  return out
}

export function startFileWatcher(listener: FileChangeListener): () => void {
  const roots = initialRoots()
  const unsubs: Array<() => void> = []
  const pendingUpserts = new Map<string, IndexedDocument>()
  const pendingRemovals = new Set<string>()
  let flushTimer: NodeJS.Timeout | null = null

  const flush = (): void => {
    flushTimer = null
    if (pendingUpserts.size === 0 && pendingRemovals.size === 0) return
    listener({
      upserts: Array.from(pendingUpserts.values()),
      removeIds: Array.from(pendingRemovals),
    })
    pendingUpserts.clear()
    pendingRemovals.clear()
  }

  const scheduleFlush = (): void => {
    if (flushTimer) return
    flushTimer = setTimeout(flush, 200)
    flushTimer.unref()
  }

  for (const root of roots) {
    try {
      const watcher = watch(root, { recursive: true }, (_event, filename) => {
        if (!filename) return
        const relative = filename.toString()
        if (containsSkippedDirectory(relative)) return
        const absolute = join(root, relative)
        const doc = makeFileDocument(absolute)
        if (doc) {
          pendingRemovals.delete(doc.id)
          pendingUpserts.set(doc.id, doc)
          scheduleFlush()
          return
        }
        if (!existsSync(absolute)) {
          const id = `file:${absolute}`
          pendingUpserts.delete(id)
          pendingRemovals.add(id)
          scheduleFlush()
        }
      })
      unsubs.push(() => watcher.close())
    } catch {
      // If recursive watch is unsupported, we still have Spotlight fallback.
    }
  }

  return () => {
    if (flushTimer) clearTimeout(flushTimer)
    flush()
    for (const stop of unsubs) stop()
  }
}

export async function spotlightFallback(query: string, limit = 8): Promise<SearchResult[]> {
  const trimmed = query.trim()
  if (!trimmed) return []

  // Internal paths that should never surface as file results — they belong
  // to extension packages, caches, or other app-managed directories.
  const INTERNAL_PATH_PATTERNS = [
    '/extension-registry/',
    '/extensions/packages/',
    '/HTTPStorages/',
    '/Application Support/tezbar/',
  ]

  try {
    const { stdout } = await execFileAsync('mdfind', ['-name', trimmed, '-onlyin', homedir()])
    return stdout
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
      .filter((path) => !INTERNAL_PATH_PATTERNS.some((pattern) => path.includes(pattern)))
      .slice(0, limit)
      .map((path, index) => ({
        id: `spotlight:${path}`,
        title: path.split('/').pop() ?? path,
        subtitle: path,
        category: 'files' as const,
        score: 150 - index,
        action: { type: 'open-file', path },
      }))
  } catch {
    return []
  }
}
