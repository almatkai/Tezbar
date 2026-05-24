import { app, BrowserWindow, clipboard, shell } from 'electron'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import type {
  ExtensionCommandArgument,
  OpenPortProcess,
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
import { getNativeCommand } from '../nativeCommands/registry'
import { getSafetyDryRun } from '../llm/configStore'
import { confirmSafetyAction } from '../safety/confirm'
import { recordSafetyEntry } from '../safety/log'
import { getSafetyDescriptor } from '../safety/registry'
import { commandBus } from './commandBus'
// Fix imports from indexDb
import { getInstance, readBenchmarkHistory, runOfflineBenchmarks, type SearchIndexRow } from './indexDb'
import { SearchIndexDatabase } from './indexDb'
import { appsProvider } from './providers/appsProvider'
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
import { parseSearchIntent } from './queryIntent'
import { computeWeightedScore, shouldPreferRecent } from './ranker'

const execFileAsync = promisify(execFile)
const MAX_RESULTS = 80
const PROVIDER_REFRESH_MS = 90_000

// Use singleton database with session caching
const indexDb = getInstance()

let bootstrapPromise: Promise<void> | null = null
let stopFileWatcher: (() => void) | null = null
let providerRefreshTimer: NodeJS.Timeout | null = null
let lastExtensionRefreshAt = 0

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

/** Run a destructive action through the safety layer: confirmation dialog +
 *  structured log entry. Returns early (without executing) if the user
 *  rejects. `run` is only invoked once confirmation passes. */
async function runWithSafety<T extends SearchExecuteResult>(
  safetyId: SafetyActionId,
  context: Record<string, unknown>,
  run: () => Promise<T>,
  options?: { detailsOverride?: string; titleOverride?: string },
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

async function upsertProvider(provider: SearchProvider): Promise<void> {
  const docs = await provider.buildDocuments()
  if (provider.providerId === 'extensions') {
    indexDb?.removeDocumentsByCategory('extensions')
  }
  if (docs.length > 0) {
    indexDb?.upsertDocuments(docs)
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
  ])
}

async function refreshVolatileProviders(): Promise<void> {
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

    const fileDocs = await collectInitialFileDocuments()
    if (fileDocs.length > 0) {
      indexDb?.upsertDocuments(fileDocs)
    }

    stopFileWatcher = startFileWatcher((payload) => {
      if (payload.upsert) {
        indexDb?.upsertDocuments([payload.upsert])
      } else if (payload.removeId) {
        indexDb?.removeDocumentById(payload.removeId)
      }
    })

    if (!providerRefreshTimer) {
      providerRefreshTimer = setInterval(() => {
        void refreshVolatileProviders()
      }, PROVIDER_REFRESH_MS)
      providerRefreshTimer.unref()
    }

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
  indexDb?.removeDocumentsByCategory('quick-notes')
  // Re-index would happen via providers
  indexDb?.clearSearchCache()
}

/** Rebuild FTS rows for snippets after user CRUD. */
export async function reindexSnippets(): Promise<void> {
  await bootstrapSearchIndex()
  indexDb?.removeDocumentsByCategory('snippets')
  indexDb?.clearSearchCache()
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

  const intent = parseSearchIntent(query)

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
  await indexDb.ensureInitialized()
  await refreshVolatileProviders()

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
              'This extension command requires view runtime support. Use extension:run-command to render it in the Raymes extension surface.',
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
      const { stdout } = await execFileAsync('bash', ['-lc', action.command])
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
    result = await executeActionInner(action)
  } catch (error) {
    result = {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
    }
  }

  const actionId = actionIdFromResult(action, context?.resultId)
  indexDb.recordAction(actionId, result.ok)

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
