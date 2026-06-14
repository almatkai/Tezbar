import { app, BrowserWindow, clipboard, shell } from 'electron'
import { execFile } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, dirname, extname, join, resolve } from 'node:path'
import { promisify } from 'node:util'
import type {
  OpenPortProcess,
  PathCompletionItem,
  SearchAction,
  SearchBenchmarkReport,
  SearchCategory,
  SearchExecuteContext,
  SearchExecuteResult,
  SearchResult,
} from '../../shared/search'
import type { NativeCommandId } from '../../shared/nativeCommands'
import type { SafetyActionId } from '../../shared/safety'
import {
  executeExtensionCommandRuntime,
  getExtensionCommands,
  isUnsupportedRuntimeModeError,
  installExtension,
  listInstalledExtensions,
} from '../extensions/service'
import { executeNativeCommand } from '../nativeCommands/executor'
import { getSafetyDryRun } from '../llm/configStore'
import { confirmSafetyAction } from '../safety/confirm'
import { recordSafetyEntry } from '../safety/log'
import { getSafetyDescriptor } from '../safety/registry'
import { commandBus } from './commandBus'
import { appIconDataUrl } from '../appIcon'
// Fix imports from indexDb
import { getInstance, readBenchmarkHistory, runOfflineBenchmarks } from './indexDb'
import { appsProvider, listApplications } from './providers/appsProvider'
import { captureClipboardSnapshot, clipboardProvider } from './providers/clipboardProvider'
import { commandsProvider } from './providers/commandsProvider'
import { extensionsProvider } from './providers/extensionsProvider'
import {
  collectInitialFileDocuments,
  spotlightFallback,
  startFileWatcher,
} from './providers/filesProvider'
import { addQuickNote, notesProvider } from './providers/notesProvider'
import { quickLinksProvider } from './providers/quickLinksProvider'
import { snippetsProvider } from './providers/snippetsProvider'
import type { IndexedDocument, SearchProvider } from './providers/types'
import { computeWeightedScore, shouldPreferRecent } from './ranker'

const execFileAsync = promisify(execFile)
const MAX_RESULTS = 80
const PROVIDER_REFRESH_MIN_AGE_MS = 10_000
const FILE_INDEX_LIMIT = 4000

const SHELL_METACHAR_RE = /[;|&`$(){}[\]\n\r<>\\]/

function validateShellCommand(command: string): { ok: true } | { ok: false; message: string } {
  const trimmed = command.trim()
  if (!trimmed) return { ok: false, message: 'Empty shell command' }
  if (SHELL_METACHAR_RE.test(trimmed)) {
    return {
      ok: false,
      message: 'Shell metacharacters are not allowed in run-shell commands.',
    }
  }
  return { ok: true }
}

function safetyForAction(
  action: SearchAction,
): { id: SafetyActionId; context: Record<string, unknown> } | null {
  if (action.type === 'run-shell') {
    return { id: 'shell.run', context: { command: action.command } }
  }
  if (action.type === 'install-extension') {
    return { id: 'extension.install', context: { extensionId: action.extensionId } }
  }
  if (action.type === 'run-native-command') {
    if (action.commandId === 'empty-trash') {
      return { id: 'trash.empty', context: { commandId: action.commandId } }
    }
    if (action.commandId === 'sleep-system') {
      return { id: 'system.sleep', context: { commandId: action.commandId } }
    }
    if (action.commandId === 'quit-tezbar') {
      return { id: 'app.quit', context: { commandId: action.commandId } }
    }
    return { id: 'native.command', context: { commandId: action.commandId } }
  }
  return null
}

async function runWithSafety(
  safetyId: SafetyActionId,
  context: Record<string, unknown>,
  run: () => Promise<SearchExecuteResult>,
): Promise<SearchExecuteResult> {
  const descriptor = getSafetyDescriptor(safetyId)
  if (!descriptor) {
    return { ok: false, message: `Safety descriptor missing: ${safetyId}` }
  }

  const dryRun = getSafetyDryRun()
  const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0] ?? null
  const { accepted } = await confirmSafetyAction(window, descriptor, context, { dryRun })
  if (!accepted) {
    recordSafetyEntry({
      action: safetyId,
      title: descriptor.title,
      risk: descriptor.risk,
      ok: false,
      message: 'Cancelled by user',
      context: { ...context, dryRun },
    })
    return { ok: false, message: 'Cancelled' }
  }

  if (dryRun) {
    const message = `Dry run: would have ${descriptor.title.toLowerCase()}.`
    recordSafetyEntry({
      action: safetyId,
      title: descriptor.title,
      risk: descriptor.risk,
      ok: true,
      message,
      context: { ...context, dryRun: true },
    })
    return { ok: true, message }
  }

  const result = await run()
  recordSafetyEntry({
    action: safetyId,
    title: descriptor.title,
    risk: descriptor.risk,
    ok: result.ok,
    message: result.message,
    context,
  })
  return result
}

// Use singleton database with session caching
const indexDb = getInstance()

let bootstrapPromise: Promise<void> | null = null
let fileBootstrapPromise: Promise<void> | null = null
let volatileRefreshPromise: Promise<void> | null = null
let stopFileWatcher: (() => void) | null = null
let providerRefreshTimer: NodeJS.Timeout | null = null
let lastExtensionRefreshAt = 0
let lastVolatileRefreshAt = 0

type OpenWithUsageEntry = {
  count: number
  lastUsedAt: number
}

type OpenWithUsageStore = {
  version: 1
  keys: Record<string, Record<string, OpenWithUsageEntry>>
  aliases?: Record<string, Record<string, OpenWithUsageEntry>>
}

function isPresent<T>(value: T | null | undefined): value is T {
  return value !== null && value !== undefined
}

function uniqById(items: SearchResult[]): SearchResult[] {
  const seen = new Set<string>()
  const out: SearchResult[] = []
  for (const item of items) {
    if (seen.has(item.id)) continue
    seen.add(item.id)
    out.push(item)
  }
  return out
}

function actionIdFromResult(action: SearchAction, resultId?: string): string {
  if (resultId) return resultId

  switch (action.type) {
    case 'open-app':
      return `open-app:${action.appName}`
    case 'open-file':
      return `open-file:${action.path}`
    case 'open-with-app':
      return `open-with-app:${action.appName ?? 'default'}:${action.path}`
    case 'copy-text':
      return `copy-text:${action.text.slice(0, 64)}`
    case 'copy-and-paste-text':
      return `copy-and-paste-text:${action.text.slice(0, 64)}`
    case 'add-note':
      return `add-note:${action.text.slice(0, 64)}`
    case 'open-url':
      return `open-url:${action.url}`
    case 'install-extension':
      return `install-extension:${action.extensionId}`
    case 'run-extension-command':
      return `extcmd:${action.extensionId}:${action.commandName}`
    case 'run-shell':
      return `run-shell:${action.command}`
    case 'invoke-command':
      return `command:${action.commandId}`
    case 'run-native-command':
      return `native:${action.commandId}`
    default:
      return 'unknown-action'
  }
}

async function upsertProvider(provider: SearchProvider): Promise<void> {
  const docs = await provider.buildDocuments()
  if (provider.providerId === 'commands') {
    indexDb.removeDocumentsByCategory('commands')
    indexDb.removeDocumentsByCategory('native-command')
  } else if (provider.providerId === 'clipboard') {
    indexDb.removeDocumentsByCategory('clipboard')
  } else if (provider.providerId === 'notes') {
    indexDb.removeDocumentsByCategory('quick-notes')
  } else if (provider.providerId === 'snippets') {
    indexDb.removeDocumentsByCategory('snippets')
  } else if (provider.providerId === 'quick-links') {
    indexDb.removeDocumentsByCategory('quick-links')
  } else if (provider.providerId === 'apps') {
    indexDb.removeDocumentsByCategory('applications')
  } else if (provider.providerId === 'extensions') {
    indexDb.removeDocumentsByCategory('extensions')
  }
  if (docs.length > 0) {
    indexDb.upsertDocuments(docs)
  }
}

async function refreshAllProviders(): Promise<void> {
  await Promise.all([
    upsertProvider(commandsProvider),
    upsertProvider(clipboardProvider),
    upsertProvider(notesProvider),
    upsertProvider(snippetsProvider),
    upsertProvider(quickLinksProvider),
    upsertProvider(extensionsProvider),
    upsertProvider(appsProvider),
  ])
  indexDb.clearSearchCache()
}

async function refreshVolatileProviders(): Promise<void> {
  if (volatileRefreshPromise) return volatileRefreshPromise

  volatileRefreshPromise = (async () => {
    captureClipboardSnapshot()
    await Promise.all([
      upsertProvider(commandsProvider),
      upsertProvider(clipboardProvider),
      upsertProvider(notesProvider),
      upsertProvider(snippetsProvider),
      upsertProvider(quickLinksProvider),
    ])

    const now = Date.now()
    if (now - lastExtensionRefreshAt > 30_000) {
      lastExtensionRefreshAt = now
      await upsertProvider(extensionsProvider)
    }
    lastVolatileRefreshAt = Date.now()
    indexDb.clearSearchCache()
  })().finally(() => {
    volatileRefreshPromise = null
  })

  return volatileRefreshPromise
}

function refreshVolatileProvidersIfStale(): void {
  if (Date.now() - lastVolatileRefreshAt < PROVIDER_REFRESH_MIN_AGE_MS) return
  void refreshVolatileProviders().catch((error: unknown) => {
    console.warn('[Search] Failed to refresh providers:', error)
  })
}

function startBackgroundFileIndexing(): void {
  if (fileBootstrapPromise) return

  fileBootstrapPromise = (async () => {
    const fileDocs = await collectInitialFileDocuments(FILE_INDEX_LIMIT)
    indexDb.replaceDocumentsByCategory('files', fileDocs)

    stopFileWatcher = startFileWatcher(({ upserts, removeIds }) => {
      if (upserts.length > 0) indexDb.upsertDocuments(upserts)
      for (const removeId of removeIds) {
        indexDb.removeDocumentById(removeId)
      }
      if (upserts.length > 0 || removeIds.length > 0) {
        indexDb.clearSearchCache()
      }
    })
  })().catch((error: unknown) => {
    fileBootstrapPromise = null
    console.warn('[Search] Failed to build file index:', error)
  })
}

async function bootstrapSearchIndex(): Promise<void> {
  if (bootstrapPromise) {
    return bootstrapPromise
  }

  bootstrapPromise = (async () => {
    await indexDb.ensureInitialized()
    indexDb.removeDocumentsByCategory('clipboard')

    captureClipboardSnapshot()
    await refreshAllProviders()
    lastVolatileRefreshAt = Date.now()
    startBackgroundFileIndexing()

    // The index is now refreshed on demand via refreshVolatileProvidersIfStale
    // and by explicit invalidation after extension/snippets/notes changes.
    // The previous 90-second background loop has been removed to eliminate
    // idle churn.

    app.once('before-quit', () => {
      stopFileWatcher?.()
      stopFileWatcher = null
      if (providerRefreshTimer) {
        clearInterval(providerRefreshTimer)
        providerRefreshTimer = null
      }
    })
  })()

  return bootstrapPromise
}

/** Rebuild FTS rows for quick notes after CRUD (append/update/delete). */
export async function reindexQuickNotes(): Promise<void> {
  await bootstrapSearchIndex()
  await upsertProvider(notesProvider)
  indexDb.clearSearchCache()
}

/** Rebuild FTS rows for snippets after user CRUD. */
export async function reindexSnippets(): Promise<void> {
  await bootstrapSearchIndex()
  await upsertProvider(snippetsProvider)
  indexDb.clearSearchCache()
}

/** Rebuild extension command rows after extension install/uninstall. */
export async function reindexExtensions(): Promise<void> {
  await bootstrapSearchIndex()
  await upsertProvider(extensionsProvider)
  lastExtensionRefreshAt = Date.now()
  indexDb?.clearSearchCache()
}

/** First-class surfaces we own (internal commands, extensions, apps)
 *  should beat generic file matches when the user's query is a prefix or
 *  exact hit on their title. Without this, typing "clipboard" ranks
 *  `ClipboardView.tsx` above the actual "Clipboard History" command — which
 *  is painfully wrong.
 *
 *  The boost is large enough to overcome BM25 differences but scoped to
 *  internal command-shaped results so it never displaces highly-specific
 *  file matches on unrelated queries. */
function internalSurfaceBoost(
  category: SearchResult['category'],
  title: string,
  query: string,
  subtitle?: string,
): number {
  const hit =
    category === 'native-command' ||
    category === 'commands' ||
    category === 'extensions' ||
    category === 'applications' ||
    category === 'quick-notes'
  if (!hit) return 0

  const normalizedTitle = title.toLowerCase()
  const normalizedQuery = query.trim().toLowerCase()
  if (!normalizedQuery) return 0

  let boost = 0

  // Exact or prefix matches on title (commands, apps, etc.)
  if (normalizedTitle === normalizedQuery) {
    boost = 600
  } else if (normalizedTitle.startsWith(normalizedQuery)) {
    boost = 420
  } else {
    const titleWords = normalizedTitle.split(/\s+/)
    if (titleWords.some((word) => word.startsWith(normalizedQuery))) {
      boost = 300
    } else if (normalizedTitle.includes(normalizedQuery)) {
      boost = 150
    }
  }

  if (category === 'extensions' && subtitle) {
    const parts = subtitle.split(' · ')
    const extName = parts[0]?.toLowerCase()
    if (extName) {
      const slugName = extName.replace(/\s+/g, '')
      if (extName === normalizedQuery || slugName === normalizedQuery) boost = Math.max(boost, 1200)
      else if (extName.startsWith(normalizedQuery) || slugName.startsWith(normalizedQuery)) boost = Math.max(boost, 800)
      else if (extName.includes(normalizedQuery) || slugName.includes(normalizedQuery)) boost = Math.max(boost, 400)
    }
  }

  return boost
}


/** Keep recently touched quick notes near the top for a short window. */
function recentQuickNoteBoost(
  category: SearchResult['category'],
  updatedAt: number,
  now: number,
): number {
  if (category !== 'quick-notes') return 0
  const ageMs = now - updatedAt
  if (ageMs < 90_000) return 1100
  if (ageMs < 5 * 60 * 1000) return 520
  if (ageMs < 30 * 60 * 1000) return 140
  return 0
}

/** A just-saved note should win for the same query text. This closes the
 *  gap where broad OR-token matches can keep command surfaces above the
 *  note immediately after save. */
function exactRecentQuickNoteBoost(
  category: SearchResult['category'],
  title: string,
  query: string,
  updatedAt: number,
  now: number,
): number {
  if (category !== 'quick-notes') return 0
  const ageMs = now - updatedAt
  if (ageMs > 5 * 60 * 1000) return 0

  const normalizedTitle = title.trim().toLowerCase()
  const normalizedQuery = query.trim().toLowerCase()
  if (!normalizedQuery || !normalizedTitle) return 0

  if (normalizedTitle === normalizedQuery) return 1800
  if (normalizedTitle.startsWith(normalizedQuery)) return 1400
  if (normalizedTitle.includes(normalizedQuery)) return 900
  return 0
}

function rankRows(query: string, docs: Array<{ doc: IndexedDocument; lexical: number; fuzzyDistance?: number }>): Array<SearchResult & { updatedAt: number }> {
  const now = Date.now()
  const stats = indexDb?.getActionStats(docs.map((entry) => entry.doc.id)) ?? new Map()

  const ranked = docs.map((entry) => {
    const actionStat = stats.get(entry.doc.id)
    const frequency = actionStat?.frequency ?? 0
    const totalCount = actionStat?.totalCount ?? 0
    const successCount = actionStat?.successCount ?? 0
    const successRate = totalCount > 0 ? successCount / totalCount : 0
    const activityAt = actionStat?.lastUsedAt && actionStat.lastUsedAt > 0 ? actionStat.lastUsedAt : entry.doc.updatedAt

    const score =
      computeWeightedScore({
        lexical: entry.lexical,
        recencyMs: now - activityAt,
        frequency,
        successRate,
        category: entry.doc.category,
        fuzzyDistance: entry.fuzzyDistance,
        popularity: entry.doc.popularity,
      }) +
      internalSurfaceBoost(entry.doc.category, entry.doc.title, query, entry.doc.subtitle) +
      recentQuickNoteBoost(entry.doc.category, entry.doc.updatedAt, now) +
      exactRecentQuickNoteBoost(entry.doc.category, entry.doc.title, query, entry.doc.updatedAt, now) +
      (() => {
        // Quick note add row should stay competitive with strong file hits
        const q = query.trim().toLowerCase()
        if (!q) return 120
        if (/\bnotes?\b/.test(q) || q.includes('quick note')) return 780
        return 120
      })()

    return {
      id: entry.doc.id,
      title: entry.doc.title,
      subtitle: entry.doc.subtitle,
      category: entry.doc.category,
      score,
      action: entry.doc.action,
      updatedAt: activityAt,
    }
  })

  ranked.sort((left, right) => {
    if (left.score !== right.score) {
      const preferRecent = shouldPreferRecent(
        left.score,
        now - left.updatedAt,
        right.score,
        now - right.updatedAt,
      )
      if (preferRecent) return -1
      const reversePreferRecent = shouldPreferRecent(
        right.score,
        now - right.updatedAt,
        left.score,
        now - left.updatedAt,
      )
      if (reversePreferRecent) return 1
      return right.score - left.score
    }
    return right.updatedAt - left.updatedAt
  })

  return ranked
}

function recommendationBoost(id: string): number {
  if (id === 'native:open-clipboard-history') return 900
  if (id === 'native:open-snippets') return 880
  if (id === 'extcmd:raycast.kill-process:index') return 860
  if (id === 'extcmd:raycast.port-manager:kill-listening-process') return 760
  if (id === 'extcmd:raycast.port-manager:open-ports') return 720
  if (id === 'extcmd:raycast.port-manager:open-ports-menu-bar') return 700
  return 0
}

function buildRecommendations(): SearchResult[] {
  const now = Date.now()
  const seeds: Array<{
    id: string
    category: SearchCategory
    title: string
    subtitle: string
    action: SearchAction
    updatedAt: number
    frequency: number
    successRate: number
    lastUsedAt: number
  }> = indexDb.listRecommendedDocuments(MAX_RESULTS).map((row) => {
    const totalCount = row.totalCount > 0 ? row.totalCount : 0
    const successRate = totalCount > 0 ? row.successCount / totalCount : 0
    return {
      id: row.id,
      category: row.category,
      title: row.title,
      subtitle: row.subtitle,
      action: indexDb.parseAction(row.actionJson),
      updatedAt: row.updatedAt,
      frequency: row.frequency,
      successRate,
      lastUsedAt: row.lastUsedAt,
    }
  })

  const pinnedOrder = [
    'native:open-clipboard-history',
    'native:open-snippets',
    'extcmd:raycast.port-manager:kill-listening-process',
    'extcmd:raycast.kill-process:index',
    'extcmd:raycast.port-manager:open-ports',
    'extcmd:raycast.port-manager:open-ports-menu-bar',
  ]
  const pinnedRows = indexDb.getDocumentsByIds(pinnedOrder)
  const existingIds = new Set(seeds.map((seed) => seed.id))
  for (const row of pinnedRows) {
    if (existingIds.has(row.id)) continue
    seeds.push({
      id: row.id,
      category: row.category,
      title: row.title,
      subtitle: row.subtitle,
      action: indexDb.parseAction(row.actionJson),
      updatedAt: row.updatedAt,
      frequency: 0,
      successRate: 0,
      lastUsedAt: 0,
    })
  }

  return seeds
    .map((seed) => {
      const activityAt = seed.lastUsedAt > 0 ? seed.lastUsedAt : seed.updatedAt
      const score =
        computeWeightedScore({
          lexical: 0.92,
          recencyMs: now - activityAt,
          frequency: seed.frequency,
          successRate: seed.successRate,
          category: seed.category,
        }) +
        recommendationBoost(seed.id)

      return {
        id: seed.id,
        title: seed.title,
        subtitle: seed.subtitle,
        category: seed.category,
        score,
        action: seed.action,
      } satisfies SearchResult
    })
    .sort((a, b) => b.score - a.score)
    .slice(0, MAX_RESULTS)
}

function decodeLsofCommandName(value: string): string {
  return value.replace(/\\x([0-9a-fA-F]{2})/g, (_, hex: string) =>
    String.fromCharCode(Number.parseInt(hex, 16)),
  )
}

function displayProcessNameFromCommand(command: string): string {
  const trimmed = command.trim()
  if (!trimmed) return ''

  const parts = trimmed.split('/').filter(Boolean)
  return decodeLsofCommandName(parts.at(-1) ?? trimmed)
}

function parseProcessNameMap(stdout: string): Map<string, string> {
  const names = new Map<string, string>()

  for (const line of stdout.split('\n')) {
    const match = line.match(/^\s*(\d+)\s+(.+?)\s*$/)
    if (!match) continue

    const name = displayProcessNameFromCommand(match[2])
    if (name) names.set(match[1], name)
  }

  return names
}

async function readProcessNameMap(): Promise<Map<string, string>> {
  try {
    const { stdout } = await execFileAsync('/bin/ps', ['-axo', 'pid=,comm='])
    return parseProcessNameMap(stdout)
  } catch {
    return new Map<string, string>()
  }
}

function parseOpenPortProcesses(stdout: string, processNames: Map<string, string> = new Map()): OpenPortProcess[] {
  const lines = stdout
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)

  if (lines.length <= 1) return []

  const grouped = new Map<
    string,
    {
      process: string
      user: string
      pid: string
      ports: Set<number>
    }
  >()

  for (const line of lines) {
    const parts = line.split(/\s+/)
    if (parts.length < 3) continue

    const match = line.match(/:(\d+)\s+\(LISTEN\)$/)
    if (!match) continue

    const port = Number(match[1])
    if (!Number.isFinite(port)) continue

    const pid = parts[1] ?? '?'
    const process = processNames.get(pid) ?? decodeLsofCommandName(parts[0] ?? 'unknown')
    const user = parts[2] ?? 'unknown'
    const key = `${process}:${pid}:${user}`

    const existing = grouped.get(key)
    if (existing) {
      existing.ports.add(port)
      continue
    }

    grouped.set(key, {
      process,
      user,
      pid,
      ports: new Set<number>([port]),
    })
  }

  return Array.from(grouped.values())
    .map((entry) => ({
      process: entry.process,
      user: entry.user,
      pid: entry.pid,
      ports: Array.from(entry.ports).sort((a, b) => a - b),
    }))
    .sort((a, b) => a.process.localeCompare(b.process) || a.pid.localeCompare(b.pid))
}



export async function searchEverything(query: string): Promise<SearchResult[]> {
  await bootstrapSearchIndex()
  refreshVolatileProvidersIfStale()

  const trimmed = query.trim()
  if (!trimmed) {
    return buildRecommendations()
  }

  const rows = indexDb.getSearch(trimmed, MAX_RESULTS)
  const docs: Array<{ doc: IndexedDocument; lexical: number; fuzzyDistance?: number }> = rows.map((row) => ({
    doc: {
      id: row.id,
      category: row.category,
      title: row.title,
      subtitle: row.subtitle,
      tokens: `${row.title} ${row.subtitle}`,
      action: indexDb.parseAction(row.actionJson),
      updatedAt: row.updatedAt,
      popularity: row.popularity,
    },
    lexical: row.lexical,
    fuzzyDistance: row.fuzzyDistance,
  }))

  const ranked = rankRows(trimmed, docs)
  const asResults = ranked.map((item) => ({
    id: item.id,
    title: item.title,
    subtitle: item.subtitle,
    category: item.category,
    score: item.score,
    action: item.action,
  }))

  const resultsWithoutFiles = asResults.filter((result) => {
    if (result.category === 'files') return false
    return true
  })
  const fileResults = asResults.filter((result) => result.category === 'files')

  let fallbackFiles: SearchResult[] = []
  if (trimmed.length > 0 && fileResults.length < 2) {
    fallbackFiles = await spotlightFallback(trimmed)
  }

  const emojiPickerResult = buildEmojiPickerSearchResult(trimmed)
  const openPortResults = await searchPortManagerOpenPorts(trimmed)

  function quickNoteAddScore(query: string): number {
    const q = query.trim().toLowerCase()
    if (!q) return 120
    if (/\bnotes?\b/.test(q) || q.includes('quick note')) return 780
    return 120
  }

  const noteAdd = trimmed
    ? [
      {
        id: `note-add:${trimmed}`,
        title: `Add quick note: ${trimmed.slice(0, 64)}`,
        subtitle: 'Quick notes',
        category: 'quick-notes' as const,
        score: quickNoteAddScore(trimmed),
        action: { type: 'add-note', text: trimmed },
      } satisfies SearchResult,
    ]
    : []

  return uniqById([
    ...resultsWithoutFiles,
    ...emojiPickerResult,
    ...fileResults,
    ...fallbackFiles,
    ...openPortResults,
    ...noteAdd,
  ])
    .sort((a, b) => b.score - a.score)
    .slice(0, MAX_RESULTS)
}

function expandUserPath(input: string): string {
  if (input === '~') return homedir()
  if (input.startsWith('~/')) return join(homedir(), input.slice(2))
  return input
}

function resolveSlashPathInput(input: string): string {
  if (!input.startsWith('/')) return expandUserPath(input)

  const absolutePrefixes = ['/Users/', '/Volumes/', '/private/', '/tmp/', '/var/', '/System/', '/Library/']
  if (absolutePrefixes.some((prefix) => input.startsWith(prefix))) {
    return input
  }

  if (input === '/Users' || input === '/Volumes') {
    return input
  }

  return join(homedir(), input.slice(1))
}

function displayUserPath(path: string): string {
  const home = homedir()
  if (path === home) return '~'
  if (path.startsWith(`${home}/`)) return `~/${path.slice(home.length + 1)}`
  return path
}

function splitPathCompletionQuery(raw: string): {
  targetPath: string
  appTerm: string
  appMode: boolean
} {
  const query = raw.trimStart()
  const body = query === '/' ? '' : query
  const expandedBody = resolveSlashPathInput(body)

  let appMode = false
  let targetPart = expandedBody
  let appTerm = ''

  const trimmedBody = expandedBody.trimEnd()
  if (expandedBody.endsWith(' ') && trimmedBody && existsSync(trimmedBody)) {
    appMode = true
    targetPart = trimmedBody
    appTerm = ''
  } else if (!existsSync(expandedBody)) {
    let splitAt = -1
    for (let index = expandedBody.length - 1; index >= 0; index--) {
      if (expandedBody[index] !== ' ') continue
      const beforeSpace = expandedBody.slice(0, index).trimEnd()
      if (beforeSpace && existsSync(beforeSpace)) {
        splitAt = index
        break
      }
    }
    if (splitAt >= 0) {
      appMode = true
      targetPart = expandedBody.slice(0, splitAt).trimEnd()
      appTerm = expandedBody.slice(splitAt + 1).trimStart()
    }
  }

  if (!targetPart) {
    return { targetPath: homedir(), appTerm, appMode }
  }

  if (targetPart.startsWith('/')) {
    return { targetPath: targetPart, appTerm, appMode }
  }

  if (targetPart.startsWith('~')) {
    return { targetPath: expandUserPath(targetPart), appTerm, appMode }
  }

  return { targetPath: resolve(homedir(), targetPart), appTerm, appMode }
}

function pathCompletionBase(targetPath: string): { directory: string; prefix: string } {
  if (targetPath.endsWith('/')) return { directory: targetPath, prefix: '' }
  try {
    if (existsSync(targetPath) && statSync(targetPath).isDirectory()) {
      return { directory: targetPath, prefix: '' }
    }
  } catch {
    // Fall through to dirname/prefix parsing.
  }
  return { directory: dirname(targetPath), prefix: basename(targetPath) }
}

function openWithUsageStorePath(): string {
  return join(app.getPath('userData'), 'open-with-usage.json')
}

function readOpenWithUsageStore(): OpenWithUsageStore {
  try {
    const parsed = JSON.parse(readFileSync(openWithUsageStorePath(), 'utf8'))
    if (!parsed || typeof parsed !== 'object' || parsed.version !== 1 || typeof parsed.keys !== 'object') {
      return { version: 1, keys: {} }
    }
    if (!parsed.aliases || typeof parsed.aliases !== 'object') {
      parsed.aliases = {}
    }
    return parsed as OpenWithUsageStore
  } catch {
    return { version: 1, keys: {} }
  }
}

function writeOpenWithUsageStore(store: OpenWithUsageStore): void {
  const path = openWithUsageStorePath()
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, JSON.stringify(store), 'utf8')
}

function openWithUsageKeysForPath(targetPath: string): string[] {
  try {
    const stat = statSync(targetPath)
    if (stat.isDirectory()) {
      return [`folder:${targetPath}`, `sibling-folder:${dirname(targetPath)}`]
    }
  } catch {
    // Missing paths still fall through to extension-based hints.
  }

  const ext = extname(targetPath).toLowerCase()
  const parent = dirname(targetPath)
  const keys = [`parent:${parent}`]
  if (ext) {
    keys.push(`parent-ext:${parent}:${ext}`, `ext:${ext}`)
  }
  return keys
}

function recordOpenWithUsage(targetPath: string, appName: string): void {
  const cleanAppName = appName.trim()
  if (!targetPath || !cleanAppName) return

  try {
    const store = readOpenWithUsageStore()
    const now = Date.now()
    for (const key of openWithUsageKeysForPath(targetPath)) {
      const bucket = (store.keys[key] ??= {})
      const existing = bucket[cleanAppName]
      bucket[cleanAppName] = {
        count: (existing?.count ?? 0) + 1,
        lastUsedAt: now,
      }
    }
    writeOpenWithUsageStore(store)
  } catch (error) {
    console.warn('[Search] Failed to record open-with usage:', error)
  }
}

function normalizeAppSearchTerm(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/&/g, 'and')
    .replace(/[^a-z0-9]+/g, '')
}

function appAcronym(name: string): string {
  return name
    .split(/[^a-zA-Z0-9]+/)
    .filter(Boolean)
    .map((part) => part[0]?.toLowerCase() ?? '')
    .join('')
}

function builtInAppAliases(appName: string): string[] {
  const n = appName.toLowerCase()
  if (n === 'visual studio code') return ['vscode', 'vsc', 'code', 'vs code']
  if (n === 'quicktime player') return ['quicktime', 'qt', 'qtp']
  if (n === 'activity monitor') return ['activity', 'taskmanager', 'task monitor']
  if (n === 'terminal') return ['term', 'shell']
  if (n === 'finder') return ['files', 'filemanager']
  return []
}

function appMatchesTerm(appName: string, term: string, learnedAliases?: Record<string, OpenWithUsageEntry>): boolean {
  const normalizedTerm = normalizeAppSearchTerm(term)
  if (!normalizedTerm) return true

  const normalizedName = normalizeAppSearchTerm(appName)
  if (normalizedName.includes(normalizedTerm)) return true
  if (appAcronym(appName).includes(normalizedTerm)) return true
  if (builtInAppAliases(appName).some((alias) => normalizeAppSearchTerm(alias).includes(normalizedTerm))) {
    return true
  }
  return Boolean(learnedAliases?.[appName])
}

function recordOpenWithAlias(term: string, appName: string): void {
  const alias = normalizeAppSearchTerm(term)
  const cleanAppName = appName.trim()
  if (!alias || !cleanAppName) return
  if (normalizeAppSearchTerm(cleanAppName).includes(alias)) return

  try {
    const store = readOpenWithUsageStore()
    const aliases = (store.aliases ??= {})
    const bucket = (aliases[alias] ??= {})
    const existing = bucket[cleanAppName]
    bucket[cleanAppName] = {
      count: (existing?.count ?? 0) + 1,
      lastUsedAt: Date.now(),
    }
    writeOpenWithUsageStore(store)
  } catch (error) {
    console.warn('[Search] Failed to record open-with alias:', error)
  }
}

function learnedAliasScores(term: string): Record<string, OpenWithUsageEntry> | undefined {
  const alias = normalizeAppSearchTerm(term)
  if (!alias) return undefined
  return readOpenWithUsageStore().aliases?.[alias]
}

function recommendedOpenWithApps(targetPath: string): Array<{ appName: string; score: number }> {
  const store = readOpenWithUsageStore()
  const weights = new Map<string, number>()
  const now = Date.now()

  openWithUsageKeysForPath(targetPath).forEach((key, index) => {
    const bucket = store.keys[key]
    if (!bucket) return
    const keyWeight = index === 0 ? 5 : index === 1 ? 3 : 1
    for (const [appName, entry] of Object.entries(bucket)) {
      const ageDays = Math.max(0, (now - entry.lastUsedAt) / 86_400_000)
      const recencyBoost = Math.max(0, 14 - ageDays)
      weights.set(appName, (weights.get(appName) ?? 0) + keyWeight * entry.count + recencyBoost)
    }
  })

  return Array.from(weights.entries())
    .map(([appName, score]) => ({ appName, score }))
    .sort((a, b) => b.score - a.score || a.appName.localeCompare(b.appName))
}

function isApplicationsDirectory(path: string): boolean {
  const normalized = path.replace(/\/+$/, '')
  return (
    normalized === '/Applications' ||
    normalized === '/System/Applications' ||
    normalized === '/System/Applications/Utilities' ||
    normalized === join(homedir(), 'Applications')
  )
}

function inferredDefaultAppName(targetPath: string): string {
  try {
    if (statSync(targetPath).isDirectory()) return 'Finder'
  } catch {
    // Fall through to extension/name heuristics.
  }

  const ext = extname(targetPath).toLowerCase()
  const parent = dirname(targetPath).toLowerCase()
  if (/\b(movie|movies|video|videos)\b/.test(parent) && ['.ts', '.m2ts', '.mts'].includes(ext)) {
    return 'QuickTime Player'
  }
  if (['.png', '.jpg', '.jpeg', '.gif', '.heic', '.webp', '.tiff', '.bmp', '.pdf'].includes(ext)) {
    return 'Preview'
  }
  if (['.mov', '.mp4', '.m4v', '.avi', '.mkv', '.m2ts', '.mts'].includes(ext)) {
    return 'QuickTime Player'
  }
  return 'Default App'
}

async function applicationCompletionItem(
  targetPath: string,
  appInfo: { name: string; path: string },
  index: number,
  section: 'recommended' | 'applications',
  score: number,
): Promise<PathCompletionItem> {
  return {
    id: `path-app:${section}:${appInfo.path}`,
    title: appInfo.name,
    subtitle: `Open ${displayUserPath(targetPath)} with ${appInfo.name}`,
    kind: 'application',
    section,
    badge: section === 'recommended' ? 'Recommended' : 'Open With',
    value: `${targetPath} ${appInfo.name}`,
    path: targetPath,
    appName: appInfo.name,
    applicationAction: 'open-with',
    iconDataUrl: await appIconDataUrl(appInfo.path),
    score: score - index,
  }
}

async function installedApplicationItem(
  appInfo: { name: string; path: string },
  index: number,
): Promise<PathCompletionItem> {
  return {
    id: `path-installed-app:${appInfo.path}`,
    title: appInfo.name,
    subtitle: displayUserPath(appInfo.path),
    kind: 'application',
    badge: 'Application',
    value: appInfo.path,
    path: appInfo.path,
    appName: appInfo.name,
    applicationAction: 'open',
    iconDataUrl: await appIconDataUrl(appInfo.path),
    score: 2_000 - index,
  }
}

export async function completePath(query: string, limit = 50): Promise<PathCompletionItem[]> {
  const applicationQuery = query.trimStart()
  if (applicationQuery.startsWith('`')) {
    const appTerm = applicationQuery.slice(1).trim()
    const apps = listApplications()
      .filter((item) => appMatchesTerm(item.name, appTerm))
      .sort((a, b) => a.name.localeCompare(b.name))
    return Promise.all(apps.map((item, index) => installedApplicationItem(item, index)))
  }

  const { targetPath, appTerm, appMode } = splitPathCompletionQuery(query)

  if (!appMode && isApplicationsDirectory(targetPath)) {
    const apps = listApplications()
      .sort((a, b) => a.name.localeCompare(b.name))
      .slice(0, limit)
    return Promise.all(apps.map((item, index) => installedApplicationItem(item, index)))
  }

  if (appMode) {
    const learnedAliases = learnedAliasScores(appTerm)
    const allApps = listApplications()
      .sort((a, b) => a.name.localeCompare(b.name))
      .filter((item) => appMatchesTerm(item.name, appTerm, learnedAliases))
    const appsByName = new Map(allApps.map((item) => [item.name, item]))
    const learnedRecommended = Object.entries(learnedAliases ?? {})
      .sort((a, b) => b[1].count - a[1].count || b[1].lastUsedAt - a[1].lastUsedAt)
      .map(([appName]) => appsByName.get(appName))
      .filter(isPresent)
    const usageRecommended = recommendedOpenWithApps(targetPath)
      .map((item) => appsByName.get(item.appName))
      .filter(isPresent)
    const recommended = [...learnedRecommended, ...usageRecommended]
      .filter((item, index, items) => items.findIndex((other) => other.name === item.name) === index)
      .slice(0, 5)
    const recommendedNames = new Set(recommended.map((item) => item.name))
    const rest = allApps.filter((item) => !recommendedNames.has(item.name)).slice(0, limit)

    const recommendedItems = await Promise.all(
      recommended.map((item, index) =>
        applicationCompletionItem(targetPath, item, index, 'recommended', 4_000),
      ),
    )
    const appItems = await Promise.all(
      rest.slice(0, Math.max(0, limit - recommendedItems.length - 1)).map((item, index) =>
        applicationCompletionItem(targetPath, item, index, 'applications', 1_000),
      ),
    )
    const defaultItem = {
      id: `path-default:${targetPath}`,
      title: `Open in ${inferredDefaultAppName(targetPath)}`,
      subtitle: `Open ${displayUserPath(targetPath)}`,
      kind: 'application' as const,
      section: 'default' as const,
      badge: 'Default',
      value: `${targetPath} `,
      path: targetPath,
      applicationAction: 'open-with' as const,
      score: 2_000,
    }

    if (appTerm.trim()) {
      return [
        ...recommendedItems,
        ...appItems,
        defaultItem,
      ]
    }

    return [
      ...recommendedItems,
      defaultItem,
      ...appItems,
    ]
  }

  const { directory, prefix } = pathCompletionBase(targetPath)
  const normalizedPrefix = prefix.toLowerCase()

  try {
    const entries = readdirSync(directory, { withFileTypes: true })
    return entries
      .filter((entry) => !entry.name.startsWith('.'))
      .filter((entry) => !normalizedPrefix || entry.name.toLowerCase().includes(normalizedPrefix))
      .map<PathCompletionItem | null>((entry) => {
        const absolute = join(directory, entry.name)
        const isDirectory = entry.isDirectory()
        const isFile = entry.isFile()
        if (!isDirectory && !isFile) return null
        const kind = isDirectory ? 'directory' : 'file'
        const lowerName = entry.name.toLowerCase()
        return {
          id: `path:${absolute}`,
          title: entry.name,
          subtitle: displayUserPath(absolute),
          kind,
          value: isDirectory ? `${absolute}/` : absolute,
          path: absolute,
          score:
            (isDirectory ? 1_000 : 500) +
            (lowerName === normalizedPrefix ? 1_000 : lowerName.startsWith(normalizedPrefix) ? 100 : 0),
        }
      })
      .filter(isPresent)
      .sort((a, b) => {
        if (a.kind !== b.kind) return a.kind === 'directory' ? -1 : 1
        return b.score - a.score || a.title.localeCompare(b.title)
      })
      .slice(0, limit)
  } catch {
    return []
  }
}

async function searchPortManagerOpenPorts(query: string): Promise<SearchResult[]> {
  const normalizedQuery = query.trim().toLowerCase()
  if (!normalizedQuery) return []
  const mentionsPort = /(port|ports|open|listen|listening)/.test(normalizedQuery)
  const mentionsKill = /(kill|stop|terminate|process)/.test(normalizedQuery)

  if (!/(port|ports|open|listen|kill|\d{2,5})/.test(normalizedQuery)) {
    return []
  }

  const processes = await listOpenPorts()
  if (processes.length === 0) return []

  return processes
    .flatMap<SearchResult | null>((entry) =>
      entry.ports.map((port) => {
        let score = -1
        const processName = entry.process.toLowerCase()
        const userName = entry.user.toLowerCase()

        if (normalizedQuery.includes(String(port))) {
          score = 430
        } else if (mentionsPort || mentionsKill) {
          score = 280
        } else if (processName.includes(normalizedQuery) || userName.includes(normalizedQuery)) {
          score = 220
        }

        if (score < 0) return null
        return {
          id: `port-listener:${entry.pid}:${port}`,
          title: `Open Port ${port}`,
          subtitle: `${entry.process} (PID ${entry.pid}) · ${entry.user} · Enter to kill listener`,
          category: 'extensions' as const,
          score,
          action: {
            type: 'run-extension-command',
            extensionId: 'raycast.port-manager',
            commandName: 'kill-listening-process',
            title: 'Kill Process Listening On',
            argumentValues: { port: String(port) },
          },
        }
      }),
    )
    .filter(isPresent)
    .sort((a, b) => b.score - a.score)
    .slice(0, 12)
}

function buildEmojiPickerSearchResult(query: string): SearchResult[] {
  const n = query.trim().toLowerCase()
  if (!n) return []
  const emojiActionStats = indexDb.getActionStats(['native:open-emoji-picker']).get('native:open-emoji-picker')
  const recentUseBoost = (() => {
    if (!emojiActionStats?.lastUsedAt) return 0
    const ageMs = Date.now() - emojiActionStats.lastUsedAt
    if (ageMs < 5 * 60 * 1000) return 1000
    if (ageMs < 60 * 60 * 1000) return 550
    if (ageMs < 24 * 60 * 60 * 1000) return 180
    return 0
  })()
  const shortPrefixBoost = n === 'e' ? 920 : n.startsWith('em') ? 760 : n.startsWith('emo') ? 920 : 0
  const shouldShow =
    n.includes('emoji') ||
    n.startsWith('emo') ||
    n === 'e' ||
    n.includes('smiley') ||
    n.includes('emoticon') ||
    n.includes('symbol') ||
    n === '/emoji'
  if (!shouldShow) return []
  return [
    {
      id: 'native:open-emoji-picker',
      title: 'Emoji Picker',
      subtitle: 'Browse and copy emojis by name, mood, and category.',
      category: 'native-command',
      score: 2600 + shortPrefixBoost + recentUseBoost,
      action: { type: 'run-native-command', commandId: 'open-emoji-picker' },
    },
  ]
}

export async function listOpenPorts(): Promise<OpenPortProcess[]> {
  try {
    const { stdout } = await execFileAsync('/usr/sbin/lsof', ['-nP', '-iTCP', '-sTCP:LISTEN'])
    const processNames = await readProcessNameMap()
    return parseOpenPortProcesses(stdout, processNames)
  } catch (error) {
    console.error('[OpenPorts] Failed to list listening ports:', error)
    try {
      const { stdout } = await execFileAsync('/usr/sbin/lsof', ['-nP', '-iTCP', '-sTCP:LISTEN'])
      return parseOpenPortProcesses(stdout)
    } catch (fallbackError) {
      console.error('[OpenPorts] Fallback listing failed:', fallbackError)
      return []
    }
  }
}

async function executeActionInner(action: SearchAction): Promise<SearchExecuteResult> {
  switch (action.type) {
    case 'open-app': {
      await execFileAsync('open', ['-a', action.appName])
      return { ok: true, message: `Opened ${action.appName}` }
    }

    case 'open-file': {
      const opened = await shell.openPath(action.path)
      if (opened) {
        return { ok: false, message: opened }
      }
      return { ok: true, message: 'Opened file' }
    }

    case 'open-with-app': {
      if (action.appName) {
        await execFileAsync('open', ['-a', action.appName, action.path])
        recordOpenWithUsage(action.path, action.appName)
        return { ok: true, message: `Opened with ${action.appName}` }
      }
      const opened = await shell.openPath(action.path)
      if (opened) {
        return { ok: false, message: opened }
      }
      return { ok: true, message: 'Opened' }
    }

    case 'copy-text': {
      clipboard.writeText(action.text)
      return { ok: true, message: 'Copied to clipboard' }
    }

    case 'copy-and-paste-text': {
      clipboard.writeText(action.text)
      // Give the window time to hide before firing the paste keystroke.
      await new Promise<void>((resolve) => setTimeout(resolve, 120))
      // Deactivate the app so the previously frontmost application
      // becomes active again — otherwise Cmd+V may be captured by the
      // hidden Electron window instead of the target input field.
      app.hide()
      await new Promise<void>((resolve) => setTimeout(resolve, 50))
      await execFileAsync('osascript', [
        '-e',
        'tell application "System Events" to keystroke "v" using {command down}',
      ])
      return { ok: true, message: 'Pasted emoji' }
    }

    case 'add-note': {
      const entry = addQuickNote(action.text)
      await reindexQuickNotes()
      return entry
        ? { ok: true, message: 'Saved to Quick Notes' }
        : { ok: false, message: 'Could not save quick note' }
    }

    case 'open-url': {
      await shell.openExternal(action.url)
      return { ok: true, message: 'Opened URL' }
    }

    case 'install-extension': {
      await installExtension(action.extensionId)
      return { ok: true, message: `Installing ${action.extensionId}` }
    }

    case 'run-extension-command': {
      const argumentValues: Record<string, string> = {
        ...(action.argumentValues ?? {}),
      }

      if (action.argumentName && action.argumentValue && !argumentValues[action.argumentName]) {
        argumentValues[action.argumentName] = action.argumentValue
      }

      try {
        const result = await executeExtensionCommandRuntime(
          action.extensionId,
          action.commandName,
          argumentValues,
        )
        return result
      } catch (error) {
        if (isUnsupportedRuntimeModeError(error)) {
          return {
            ok: false,
            message:
              'This extension command requires view runtime support. Use extension:run-command to render it in the TezBar extension surface.',
          }
        }
        throw error
      }
    }

    case 'invoke-command': {
      return commandBus.execute({
        commandId: action.commandId,
        payload: action.payload,
      })
    }

    case 'run-shell': {
      const command = String(action.command ?? '').trim()
      const validation = validateShellCommand(command)
      if (!validation.ok) {
        return { ok: false, message: validation.message }
      }
      const { stdout } = await execFileAsync('bash', ['-lc', command])
      const message = stdout.trim()
      return { ok: true, message: message || 'Command completed' }
    }

    case 'run-native-command': {
      return executeNativeCommand(action.commandId as NativeCommandId)
    }

    default: {
      return { ok: false, message: 'Unsupported action type' }
    }
  }
}

let _benchmarkPromise: Promise<void> | null = null

export async function runSearchBenchmarks(): Promise<void> {
  if (_benchmarkPromise) {
    return _benchmarkPromise
  }

  _benchmarkPromise = (async () => {
    await indexDb.ensureInitialized()
    await runOfflineBenchmarks(searchEverything, indexDb)
  })()

  return _benchmarkPromise
}

export async function getSearchBenchmarkHistory(): Promise<SearchBenchmarkReport[]> {
  return readBenchmarkHistory()
}

export async function executeSearchAction(
  action: SearchAction,
  context?: SearchExecuteContext,
): Promise<SearchExecuteResult> {
  let result: SearchExecuteResult

  try {
    const safety = safetyForAction(action)
    result = safety
      ? await runWithSafety(safety.id, safety.context, () => executeActionInner(action))
      : await executeActionInner(action)
  } catch (error) {
    result = {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
    }
  }

  const actionId = actionIdFromResult(action, context?.resultId)
  indexDb.recordAction(actionId, result.ok)

  if (result.ok && action.type === 'open-with-app' && action.appName && context?.query) {
    const parsed = splitPathCompletionQuery(context.query)
    if (parsed.appMode && parsed.appTerm) {
      recordOpenWithAlias(parsed.appTerm, action.appName)
    }
  }

  if (context?.query && typeof context.rank === "number" && Number.isFinite(context.rank)) {
    indexDb.recordClick(context.query, actionId, context.rank, result.ok)
  }

  return result
}

export async function listExtensionCommandIndexIds(): Promise<string[]> {
  const installed = listInstalledExtensions()
  if (installed.length === 0) return []

  const ids: string[] = []
  for (const ext of installed.slice(0, 25)) {
    const commands = await getExtensionCommands(ext.id)
    for (const cmd of commands) {
      ids.push(`extcmd:${ext.id}:${cmd.name}`)
    }
  }
  return ids
}
