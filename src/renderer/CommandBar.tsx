import {
  lazy,
  Suspense,
  type FormEvent,
  type PointerEvent,
  type ReactNode,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { createPortal } from 'react-dom'
import {
  defaultModels,
  normalizeProviderModelList,
  recommendedModel,
} from '../shared/aiProviders'
import type { LlmConfigRecord, ProviderId } from '../shared/llmConfig'
import type { IconAssetKind, PathCompletionItem, SearchResult } from '../shared/search'
import type { NativeCommandId } from '../shared/nativeCommands'
import type { ExtensionRunCommandResult } from '../shared/extensionRuntime'
import { Hint, HintBar, Kbd, Message, SelectField, TextField, cx } from './ui/primitives'
import { setCommandSurfaceEscapeConsumer } from './escapeGate'
import { GlideList } from './ui/GlideList'
import { RollingText } from './ui/RollingText'
import { useHoldToSpeak } from './hooks/useHoldToSpeak'
import { evaluateExpression, type CalcResult } from './calculator'
import { buildColorConversionResults } from './colorConverter'
import { parseCurrencyQuery } from './currency/parseCurrencyQuery'
import type { ChatSessionSummary } from '../shared/chat'
import type { AiChatBoot } from '../shared/aiChatSurface'
import { RAYMES_QUICK_NOTE_SHORTCUT_EVENT } from '../shared/aiChatSurface'
import {
  compactTerminalPath,
  isAbsoluteTerminalPath,
  type TerminalKeepAliveFor,
  type TerminalPromptInfo,
  type TerminalSaveFor,
  type TerminalSessionSummary,
} from '../shared/terminal'
import {
  readTerminalDefaults,
  writeTerminalDefaults,
  type TerminalDefaults,
} from './terminalPreferences'
import { getPreferredDefaultTarget } from './currency/currencyPreferences'
import { useCurrencyConversion } from './hooks/useCurrencyConversion'
import { ModelPicker } from './ModelPicker'
import { moveTerminalSelectionDown, terminalSessionAtIndex } from './terminalSessionSelection'
import BackgroundTaskStatus from './BackgroundTaskStatus'
import {
  addLauncherQueryHistoryEntry,
  launcherQueryHistoryEntry,
  parseLauncherQueryHistory,
  shouldRecallLastLauncherQuery,
} from './launcherQueryHistory'
import { optimisticSearchResults } from './optimisticSearch'
import {
  canQuickLookSearchResult,
  isPlainSpaceKey,
  quickLookPathsForSearchResults,
  toggleDeepSearchResultNavigation,
} from './searchResultPreview'
import {
  ACTIVATE_DEEP_SEARCH_COMMAND,
  buildDeepSearchRecommendation,
  DEEP_SEARCH_RESULT_PREFIX,
  deepSearchDraftInput,
  deepSearchInput,
  parseSearchQuery,
  searchRequestInput,
} from '../shared/searchMode'

const RECENT_EXTENSION_COMMANDS_KEY = 'tezbar:recent-extension-commands'
const RECENT_EXTENSION_COMMANDS_LIMIT = 20
const PINNED_COMMANDS_KEY = 'tezbar:pinned-commands'
const LAUNCHER_QUERY_HISTORY_KEY = 'tezbar:launcher-query-history:v1'
const HOME_SEARCH_RESULTS_CACHE_KEY = 'tezbar:home-search-results:v1'
const SEARCH_CANDIDATES_CACHE_KEY = 'tezbar:search-candidates:v1'
const MAX_CACHED_HOME_RESULTS = 40
const MAX_OPTIMISTIC_SEARCH_CANDIDATES = 1_000
const QUERY_RESULTS_CACHE_TTL_MS = 5_000
const PINNED_HOME_RESULT_SCORE_PENALTY = 850
const MAX_PINNED_COMMANDS = 12
const SEARCH_RESULT_PIN_DRAG_THRESHOLD = 6
const COMMAND_HINT = { shortcut: '>', label: 'Open terminal' } as const
const COMMAND_HINTS = [
  { shortcut: '/directory', label: 'Search files and folders' },
  { shortcut: '`', label: 'Browse applications' },
  { shortcut: '!', label: 'Deep Search file contents' },
  { shortcut: 'SPACE', label: 'Enter AI Space' },
  COMMAND_HINT,
] as const

// Columns used by the Launchpad-style applications grid (shown when typing `).
const APPLICATIONS_GRID_COLUMNS = 7

const TERMINAL_PINNED_SESSIONS_KEY = 'tezbar:terminal-pinned-sessions'
const Markdown = lazy(() =>
  import('./ui/Markdown').then(({ Markdown: MarkdownView }) => ({ default: MarkdownView }))
)

function readPinnedTerminalSessionIds(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(TERMINAL_PINNED_SESSIONS_KEY) ?? '[]')
    return Array.isArray(value)
      ? value.filter((item): item is string => typeof item === 'string')
      : []
  } catch {
    return []
  }
}

function writePinnedTerminalSessionIds(ids: string[]): void {
  try {
    localStorage.setItem(TERMINAL_PINNED_SESSIONS_KEY, JSON.stringify(ids))
  } catch {
    // Storage can be unavailable in private/webview contexts.
  }
}

function readLauncherQueryHistory(): string[] {
  try {
    return parseLauncherQueryHistory(localStorage.getItem(LAUNCHER_QUERY_HISTORY_KEY))
  } catch {
    return []
  }
}

function writeLauncherQueryHistory(history: string[]): void {
  try {
    localStorage.setItem(LAUNCHER_QUERY_HISTORY_KEY, JSON.stringify(history))
  } catch {
    // Storage can be unavailable in private/webview contexts.
  }
}

function terminalSessionSubtitle(session: TerminalSessionSummary): string {
  const command = session.lastCommand ?? session.initialCommand
  const commandSuffix = command ? ` · ${command}` : ''
  return `${compactTerminalPath(session.cwd)}${commandSuffix}`
}

function terminalSessionAge(ts: number): string {
  const delta = Math.max(0, Date.now() - ts)
  const minutes = Math.floor(delta / 60_000)
  if (minutes < 1) return 'now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  return `${Math.floor(hours / 24)}d ago`
}

type PinnedCommand = {
  id: string
  title: string
  subtitle: string
  category: SearchResult['category']
  action: SearchResult['action']
  iconDataUrl?: string
  /** ⌥ + number-row key derived from the pin's current order (10→0, 11→-, 12→=). */
  slot: number
}

type PinnedCommandTooltip = {
  id: string
  title: string
  subtitle: string
  shortcut: string
  left: number
  top: number
}

type PendingExtensionArgument = {
  name: string
  required?: boolean
  type?: string
  placeholder?: string
  title?: string
  data?: Array<{ title?: string; value?: string }>
}

type ExtensionRuntimeViewPayload = Extract<ExtensionRunCommandResult, { ok: true; mode: 'view' }>

function isPendingColorConversionAction(action: { commandName: string; title: string }): boolean {
  const haystack = `${action.title} ${action.commandName}`.toLowerCase()
  return haystack.includes('convert') && haystack.includes('color')
}

function isAiPoweredExtensionAction(action: { commandName: string; title: string }): boolean {
  const commandName = action.commandName.toLowerCase()
  const title = action.title.toLowerCase()
  return commandName === 'generate-colors' || title === 'generate colors'
}

function buildRecentExtensionCommandId(extensionId: string, commandName: string): string {
  return `extcmd:${extensionId}:${commandName}`
}

function formatExtensionRunError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error || '')
  const missingModule = /Cannot find module ['"]([^'"]+)['"]/i.exec(message)
  if (missingModule?.[1]) {
    return `Extension dependency missing: ${missingModule[1]}. Reinstall the extension and try again.`
  }
  return message.split(/\r?\n/)[0] || 'Extension command failed'
}

function readRecentExtensionCommands(): string[] {
  try {
    const raw = window.localStorage.getItem(RECENT_EXTENSION_COMMANDS_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed
      .map((value) => String(value || '').trim())
      .filter((value) => value.startsWith('extcmd:'))
      .slice(0, RECENT_EXTENSION_COMMANDS_LIMIT)
  } catch {
    return []
  }
}

function writeRecentExtensionCommands(next: string[]): void {
  window.localStorage.setItem(
    RECENT_EXTENSION_COMMANDS_KEY,
    JSON.stringify(next.slice(0, RECENT_EXTENSION_COMMANDS_LIMIT))
  )
}

function readPinnedCommands(): PinnedCommand[] {
  try {
    const raw = window.localStorage.getItem(PINNED_COMMANDS_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []

    const drafts = parsed
      .map((item) => {
        const id = String(item?.id ?? '').trim()
        const title = String(item?.title ?? '').trim()
        const subtitle = String(item?.subtitle ?? '').trim()
        const category = item?.category
        const action = item?.action
        const hasValidAction =
          typeof action === 'object' &&
          action !== null &&
          typeof (action as { type?: unknown }).type === 'string'

        if (!id || !title || typeof category !== 'string' || !hasValidAction) {
          return null
        }

        const rawSlot = item?.slot
        const slotNum =
          typeof rawSlot === 'number' && rawSlot >= 1 && rawSlot <= MAX_PINNED_COMMANDS
            ? Math.floor(rawSlot)
            : undefined
        const iconDataUrl =
          typeof item?.iconDataUrl === 'string' && item.iconDataUrl.trim()
            ? item.iconDataUrl
            : undefined

        return {
          id,
          title,
          subtitle,
          category: category as SearchResult['category'],
          action: action as SearchResult['action'],
          ...(iconDataUrl ? { iconDataUrl } : {}),
          ...(slotNum !== undefined ? { slot: slotNum } : {}),
        } satisfies PinnedCommandDraft
      })
      .filter((item): item is PinnedCommandDraft => item !== null)
      .slice(0, MAX_PINNED_COMMANDS)

    return normalizePinnedSlots(drafts)
  } catch {
    return []
  }
}

function readCachedHomeSearchResults(): SearchResult[] {
  try {
    const raw = window.localStorage.getItem(HOME_SEARCH_RESULTS_CACHE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    return parsed
      .filter((item): item is SearchResult => {
        if (!item || typeof item !== 'object') return false
        const candidate = item as Partial<SearchResult>
        return (
          typeof candidate.id === 'string' &&
          Boolean(candidate.id) &&
          typeof candidate.title === 'string' &&
          typeof candidate.subtitle === 'string' &&
          typeof candidate.category === 'string' &&
          typeof candidate.score === 'number' &&
          Number.isFinite(candidate.score) &&
          Boolean(candidate.action) &&
          typeof candidate.action === 'object' &&
          typeof (candidate.action as { type?: unknown }).type === 'string' &&
          candidate.category !== 'knowledge'
        )
      })
      .slice(0, MAX_CACHED_HOME_RESULTS)
  } catch {
    return []
  }
}

function writeCachedHomeSearchResults(results: SearchResult[]): void {
  try {
    // Icons are resolved lazily and can be large data URLs, so only cache the
    // lightweight, immediately renderable command metadata.
    const cacheable = results
      .slice(0, MAX_CACHED_HOME_RESULTS)
      .map((item) => ({ ...item, iconDataUrl: undefined }))
    window.localStorage.setItem(HOME_SEARCH_RESULTS_CACHE_KEY, JSON.stringify(cacheable))
  } catch {
    // A usable in-memory result set is still better than failing the search
    // when WebView storage is unavailable or full.
  }
}

function readCachedSearchCandidates(): SearchResult[] {
  try {
    const raw = window.localStorage.getItem(SEARCH_CANDIDATES_CACHE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    return parsed
      .filter((item): item is SearchResult => {
        if (!item || typeof item !== 'object') return false
        const candidate = item as Partial<SearchResult>
        return (
          typeof candidate.id === 'string' &&
          Boolean(candidate.id) &&
          typeof candidate.title === 'string' &&
          typeof candidate.subtitle === 'string' &&
          typeof candidate.category === 'string' &&
          typeof candidate.score === 'number' &&
          Number.isFinite(candidate.score) &&
          Boolean(candidate.action) &&
          typeof candidate.action === 'object' &&
          typeof (candidate.action as { type?: unknown }).type === 'string' &&
          candidate.category !== 'knowledge'
        )
      })
      .slice(0, MAX_OPTIMISTIC_SEARCH_CANDIDATES)
  } catch {
    return []
  }
}

function writeCachedSearchCandidates(results: SearchResult[]): void {
  try {
    const cacheable = results
      .slice(0, MAX_OPTIMISTIC_SEARCH_CANDIDATES)
      .map((item) => ({ ...item, iconDataUrl: undefined }))
    window.localStorage.setItem(SEARCH_CANDIDATES_CACHE_KEY, JSON.stringify(cacheable))
  } catch {
    // The backend query remains available if WebView storage is unavailable.
  }
}

function readInitialSearchResults(): SearchResult[] {
  const cached = readCachedHomeSearchResults()
  if (cached.length > 0) return cached
  return readPinnedCommands().map((pin, index) => ({
    id: pin.id,
    title: pin.title,
    subtitle: pin.subtitle,
    category: pin.category,
    action: pin.action,
    iconDataUrl: pin.iconDataUrl,
    score: 10_000 - index,
  }))
}

function mergeSearchCandidates(current: SearchResult[], incoming: SearchResult[]): SearchResult[] {
  const byId = new Map<string, SearchResult>()
  for (const item of incoming) byId.set(item.id, item)
  for (const item of current) {
    if (!byId.has(item.id)) byId.set(item.id, item)
  }
  return Array.from(byId.values()).slice(0, MAX_OPTIMISTIC_SEARCH_CANDIDATES)
}

function writePinnedCommands(next: PinnedCommand[]): void {
  window.localStorage.setItem(
    PINNED_COMMANDS_KEY,
    JSON.stringify(next.slice(0, MAX_PINNED_COMMANDS))
  )
}

function parsePinnedSlotFromKeyEvent(event: KeyboardEvent): number | null {
  const fromCode = /^Digit([0-9])$/.exec(event.code)?.[1]
  if (fromCode) return fromCode === '0' ? 10 : Number(fromCode)
  if (event.code === 'Minus') return 11
  if (event.code === 'Equal') return 12
  if (/^[1-9]$/.test(event.key)) return Number(event.key)
  if (event.key === '0') return 10
  return null
}

function pinnedSlotShortcutKey(slot: number): string {
  if (slot === 10) return '0'
  if (slot === 11) return '-'
  if (slot === 12) return '='
  return String(slot)
}

type PinnedCommandDraft = Omit<PinnedCommand, 'slot'> & { slot?: number }

function isPinnableSearchResult(result: SearchResult): boolean {
  return (
    result.category !== 'calculator' &&
    result.category !== 'color-converter' &&
    !result.id.startsWith(DEEP_SEARCH_RESULT_PREFIX)
  )
}

/** Keep shortcuts aligned with the current pin order. */
function normalizePinnedSlots(pins: PinnedCommandDraft[]): PinnedCommand[] {
  return pins.slice(0, MAX_PINNED_COMMANDS).map((pin, index) => ({ ...pin, slot: index + 1 }))
}

function reorderPinnedByInsertIndex(
  pins: PinnedCommand[],
  draggedId: string,
  insertIndex: number
): PinnedCommand[] {
  const from = pins.findIndex((p) => p.id === draggedId)
  if (from < 0) return pins
  let to = Math.max(0, Math.min(insertIndex, pins.length))
  const next = [...pins]
  const [item] = next.splice(from, 1)
  if (!item) return pins
  if (from < to) to -= 1
  if (from === to) return pins
  next.splice(to, 0, item)
  return next
}

/* Small search icon — refined, not emoji */
function SearchIcon(): JSX.Element {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
      <circle cx="6" cy="6" r="4.1" stroke="currentColor" strokeWidth="1.3" />
      <path d="m9.3 9.3 2.4 2.4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
    </svg>
  )
}

/* App Store icon used in applications (Launchpad) mode. The mark is built from
   the same three rounded strokes as Apple's icon instead of a text glyph. */
function AppStoreIcon(): JSX.Element {
  return (
    <svg width="20" height="20" viewBox="0 0 16 16" fill="none" aria-hidden>
      <defs>
        <linearGradient
          id="appstore-gradient"
          x1="8"
          y1="0.5"
          x2="8"
          y2="15.5"
          gradientUnits="userSpaceOnUse"
        >
          <stop stopColor="#4CC3FF" />
          <stop offset="0.52" stopColor="#168CF5" />
          <stop offset="1" stopColor="#0875E1" />
        </linearGradient>
      </defs>
      <rect x="0.5" y="0.5" width="15" height="15" rx="4" fill="url(#appstore-gradient)" />
      <rect
        x="0.75"
        y="0.75"
        width="14.5"
        height="14.5"
        rx="3.75"
        stroke="white"
        strokeOpacity="0.18"
        strokeWidth="0.5"
      />
      <path
        d="M3.35 11.95 8.15 3.65M7.05 3.65l5 8.3M3.05 9.45h9.9"
        stroke="white"
        strokeWidth="1.65"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/* Large rounded app tile used inside the Launchpad-style applications grid. */
function AppGridIcon({ item }: { item: PathCompletionItem }): JSX.Element {
  const iconAsset = completionIconAsset(item)
  const containerRef = useRef<HTMLSpanElement>(null)
  const [dataUrl, setDataUrl] = useState<string | undefined>(item.iconDataUrl)
  const key = iconAsset ? `${iconAsset.kind}:${iconAsset.path}` : null

  useEffect(() => {
    if (item.iconDataUrl) {
      setDataUrl(item.iconDataUrl)
      return
    }
    if (!iconAsset) return
    let cancelled = false
    const load = (): void => {
      void loadAssetIcon(iconAsset.kind, iconAsset.path).then((icon) => {
        if (!cancelled && icon) setDataUrl(icon)
      })
    }
    const element = containerRef.current
    if (!element || typeof IntersectionObserver === 'undefined') {
      load()
      return () => {
        cancelled = true
      }
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return
        observer.disconnect()
        load()
      },
      { rootMargin: '120px' }
    )
    observer.observe(element)
    return () => {
      cancelled = true
      observer.disconnect()
    }
  }, [item.iconDataUrl, iconAsset, key])

  return (
    <span
      ref={containerRef}
      aria-hidden
      className="grid h-full w-full place-items-center overflow-hidden rounded-[14px] bg-white/[0.06] shadow-md shadow-black/30 ring-1 ring-black/10"
    >
      {dataUrl ? (
        <img src={dataUrl} alt="" className="h-full w-full object-cover" draggable={false} />
      ) : (
        <span className="text-[18px] font-semibold text-ink-3">
          {item.title.charAt(0).toUpperCase()}
        </span>
      )}
    </span>
  )
}

/* Launchpad-style grid of application tiles shown when browsing apps (typing `). */
function ApplicationGrid({
  items,
  selectedIndex,
  columns,
  onHover,
  onActivate,
}: {
  items: PathCompletionItem[]
  selectedIndex: number
  columns: number
  onHover: (index: number) => void
  onActivate: (item: PathCompletionItem) => void
}): JSX.Element {
  return (
    <div
      className="grid min-h-0 flex-1 content-start gap-x-1 gap-y-4 overflow-y-auto px-3 py-3"
      style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
    >
      {items.map((item, i) => {
        const active = i === selectedIndex
        return (
          <button
            key={item.id}
            type="button"
            onMouseMove={() => onHover(i)}
            onMouseDown={(ev) => ev.preventDefault()}
            onClick={() => onActivate(item)}
            className={cx(
              'group flex flex-col items-center gap-1.5 rounded-xl px-1 py-1.5 text-center transition',
              active ? 'bg-white/10 ring-1 ring-white/25' : 'hover:bg-white/[0.06]'
            )}
          >
            <span className="h-12 w-12 sm:h-14 sm:w-14">
              <AppGridIcon item={item} />
            </span>
            <span className="w-full truncate px-0.5 text-[10.5px] font-medium leading-tight text-ink-3 group-hover:text-ink-1">
              {item.title}
            </span>
          </button>
        )
      })}
    </div>
  )
}

function PortArgumentChip(): ReactNode {
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center gap-1.5 rounded-tezbar-chip border border-emerald-400/35 bg-emerald-500/15 px-2 py-1 text-[13px] font-semibold text-emerald-100"
    >
      <span className="grid h-4 w-4 place-items-center rounded-[4px] bg-emerald-400/25 text-emerald-100">
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden>
          <path
            d="M2.5 8.8 8.8 2.5M4.5 9.5H2v-2.5M7.5 2H10v2.5"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>
      Port
    </span>
  )
}

function InlineArgumentChip({ label }: { label: string }): ReactNode {
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center gap-1.5 rounded-tezbar-chip border border-emerald-400/35 bg-emerald-500/15 px-2 py-1 text-[13px] font-semibold text-emerald-100"
    >
      <span className="h-1.5 w-1.5 rounded-full bg-emerald-300" />
      {label}
    </span>
  )
}

/* AI Mode Icon */
function AiIcon(): JSX.Element {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
      <path
        d="M7 11.5c2.485 0 4.5-2.015 4.5-4.5S9.485 2.5 7 2.5 2.5 4.515 2.5 7c0 1.05.36 2.015.964 2.783L3 11l1.217-.464c.768.604 1.733.964 2.783.964z"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/* Terminal icon */
function TerminalIcon(): JSX.Element {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
      <rect x="1.5" y="1.5" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M4 5.5L6.5 7L4 8.5"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M8 9.5H10.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
    </svg>
  )
}
/* Key glyph — used by the Generate Password result card when a command has
 * no explicit NATIVE_COMMAND_ICON_BY_ID entry. Identical geometry to the
 * 'key' case in CommandIconGlyph so the card and rail never diverge. */
function KeyGlyph(): ReactNode {
  // Password-generation art: full-bleed field, thinner strokes, dots on
  // pixel centers, check swung wider for breathing room. No card tile.
  return (
    <>
      <rect x="0.4" y="3" width="13.2" height="8" rx="1.8" strokeWidth="0.8" />
      <path d="M2 7H2.02M4.7 7H4.72M7.4 7H7.42" strokeWidth="1.2" />
      <path d="M9.1 6.55 10 7.45 11.7 5.75" strokeWidth="0.95" />
    </>
  )
}

/* Tone for a native-command result card, keyed by result kind. Falls back to
 * the neutral emerald "productivity" tone so cards always read as content. */
function nativeResultCardTone(kind: string): {
  card: string
  chip: string
  text: string
  label: string
} {
  switch (kind) {
    case 'password':
      return {
        card: 'border-emerald-300/25 bg-emerald-300/[0.07] shadow-[inset_0_0_20px_rgba(52,211,153,0.05)]',
        chip: 'border-emerald-300/30 bg-emerald-300/10 text-emerald-300',
        text: 'text-emerald-50',
        label: 'text-emerald-300/80',
      }
    case 'copied':
      return {
        card: 'border-emerald-300/25 bg-emerald-300/[0.06] shadow-[inset_0_0_20px_rgba(52,211,153,0.04)]',
        chip: 'border-emerald-300/30 bg-emerald-300/10 text-emerald-300',
        text: 'text-emerald-50',
        label: 'text-emerald-300/80',
      }
    case 'toggle':
      return {
        card: 'border-lime-300/25 bg-lime-300/[0.06] shadow-[inset_0_0_20px_rgba(163,230,53,0.04)]',
        chip: 'border-lime-300/30 bg-lime-300/10 text-lime-300',
        text: 'text-lime-50',
        label: 'text-lime-300/80',
      }
    case 'info':
    default:
      return {
        card: 'border-sky-300/25 bg-sky-300/[0.06] shadow-[inset_0_0_20px_rgba(125,211,252,0.04)]',
        chip: 'border-sky-300/30 bg-sky-300/10 text-sky-300',
        text: 'text-sky-50',
        label: 'text-sky-300/80',
      }
  }
}

/* Small caption tag shown on the right of a result card. */
function nativeResultCardLabel(kind: string): string | null {
  switch (kind) {
    case 'password':
    case 'copied':
      return 'copied'
    default:
      return null
  }
}

type ListItemIconKind = PathCompletionItem['kind'] | SearchResult['category']

type CommandIconKind =
  | 'settings'
  | 'deep-search'
  | 'indexing'
  | 'extensions'
  | 'snippets'
  | 'notes'
  | 'emoji'
  | 'clipboard'
  | 'terminal'
  | 'key'
  | 'moon'
  | 'display'
  | 'display-sleep'
  | 'volume'
  | 'volume-muted'
  | 'desktop'
  | 'dock'
  | 'menu-bar'
  | 'finder'
  | 'awake'
  | 'sleep'
  | 'bluetooth'
  | 'wifi'
  | 'network'
  | 'ip'
  | 'dns'
  | 'vpn'
  | 'trash'
  | 'lock'
  | 'folder-downloads'
  | 'applications'
  | 'library'
  | 'copy-path'
  | 'quit'
  | 'macos'
  | 'cpu'
  | 'memory'
  | 'disk'
  | 'battery'
  | 'ports'
  | 'git'
  | 'brew'

type TezbarCommandId =
  | 'open-settings'
  | typeof ACTIVATE_DEEP_SEARCH_COMMAND
  | 'open-extensions-settings'
  | 'open-extensions'
  | 'open-snippets'
  | 'open-notes'
  | 'open-emoji-picker'
  | 'open-indexing'

const TEZBAR_COMMAND_ICON_BY_ID: Record<TezbarCommandId, CommandIconKind> = {
  'open-settings': 'settings',
  [ACTIVATE_DEEP_SEARCH_COMMAND]: 'deep-search',
  'open-extensions-settings': 'settings',
  'open-extensions': 'extensions',
  'open-snippets': 'snippets',
  'open-notes': 'notes',
  'open-emoji-picker': 'emoji',
  'open-indexing': 'indexing',
}

const NATIVE_COMMAND_ICON_BY_ID: Record<NativeCommandId, CommandIconKind> = {
  'toggle-dark-mode': 'moon',
  'start-screen-saver': 'display',
  'sleep-display': 'display-sleep',
  'toggle-mute': 'volume-muted',
  'volume-up': 'volume',
  'volume-down': 'volume',
  'toggle-hide-desktop-icons': 'desktop',
  'toggle-autohide-dock': 'dock',
  'toggle-autohide-menu-bar': 'menu-bar',
  'restart-dock': 'dock',
  'restart-finder': 'finder',
  'restart-menu-bar': 'menu-bar',
  'start-keep-awake': 'awake',
  'stop-keep-awake': 'sleep',
  'sleep-system': 'sleep',
  'toggle-bluetooth': 'bluetooth',
  'toggle-wifi': 'wifi',
  'show-network-info': 'network',
  'show-public-ip': 'ip',
  'flush-dns-cache': 'dns',
  'toggle-vpn-menu': 'vpn',
  'empty-trash': 'trash',
  'lock-screen': 'lock',
  'open-downloads': 'folder-downloads',
  'open-applications': 'applications',
  'reveal-library': 'library',
  'copy-current-path': 'copy-path',
  'quit-tezbar': 'quit',
  'show-macos-version': 'macos',
  'show-cpu-info': 'cpu',
  'show-system-monitor': 'cpu',
  'show-memory-info': 'memory',
  'show-disk-usage': 'disk',
  'show-battery-status': 'battery',
  'list-listening-ports': 'ports',
  'git-root': 'git',
  'brew-outdated': 'brew',
  'brew-update': 'brew',
  'open-clipboard-history': 'clipboard',
  'open-snippets': 'snippets',
  'open-quick-notes': 'notes',
  'open-emoji-picker': 'emoji',
  'generate-password': 'key',
}

function commandIconForResult(item: SearchResult): CommandIconKind | undefined {
  if (item.action.type === 'invoke-command') {
    return TEZBAR_COMMAND_ICON_BY_ID[item.action.commandId as TezbarCommandId]
  }
  if (item.action.type === 'run-native-command') {
    return NATIVE_COMMAND_ICON_BY_ID[item.action.commandId as NativeCommandId]
  }
  return undefined
}

function commandIconTone(kind: CommandIconKind): string {
  switch (kind) {
    case 'settings':
    case 'terminal':
    case 'macos':
    case 'cpu':
    case 'memory':
    case 'disk':
    case 'ports':
    case 'indexing':
      return 'border-sky-300/25 bg-sky-300/10 text-sky-200'
    case 'deep-search':
      return 'border-cyan-300/30 bg-cyan-300/[0.12] text-cyan-200 shadow-[inset_0_1px_0_rgba(255,255,255,0.06),0_0_12px_rgba(103,232,249,0.08)]'
    case 'extensions':
    case 'brew':
    case 'git':
      return 'border-amber-300/25 bg-amber-300/10 text-amber-200'
    case 'snippets':
    case 'notes':
    case 'clipboard':
    case 'emoji':
    case 'key':
      return 'border-emerald-300/25 bg-emerald-300/10 text-emerald-200'
    case 'moon':
    case 'display':
    case 'display-sleep':
    case 'desktop':
    case 'dock':
    case 'menu-bar':
    case 'finder':
    case 'applications':
    case 'library':
    case 'folder-downloads':
    case 'copy-path':
      return 'border-violet-300/25 bg-violet-300/10 text-violet-200'
    case 'volume':
    case 'volume-muted':
    case 'bluetooth':
    case 'wifi':
    case 'network':
    case 'ip':
    case 'dns':
    case 'vpn':
      return 'border-cyan-300/25 bg-cyan-300/10 text-cyan-200'
    case 'awake':
    case 'sleep':
    case 'battery':
      return 'border-lime-300/25 bg-lime-300/10 text-lime-200'
    case 'trash':
    case 'lock':
    case 'quit':
      return 'border-rose-300/25 bg-rose-300/10 text-rose-200'
    default:
      return 'border-white/10 bg-white/[0.04] text-ink-3'
  }
}

function CommandIconGlyph({ kind }: { kind: CommandIconKind }): ReactNode {
  switch (kind) {
    case 'deep-search':
      return (
        <>
          <path d="M2.25 2.25h6.5M2.25 4.75H6.5M2.25 7.25h2.5" />
          <circle cx="8.25" cy="8.25" r="3" />
          <path d="m10.45 10.45 1.8 1.8" />
          <path d="M8.25 6.85v2.8M6.85 8.25h2.8" strokeWidth=".9" />
        </>
      )
    case 'settings':
      return (
        <g fill="currentColor" stroke="none" transform="scale(0.583333)">
          <path d="M12 4a1 1 0 0 0-1 1c0 1.692-2.046 2.54-3.243 1.343a1 1 0 1 0-1.414 1.414C7.54 8.954 6.693 11 5 11a1 1 0 1 0 0 2c1.692 0 2.54 2.046 1.343 3.243a1 1 0 0 0 1.414 1.414C8.954 16.46 11 17.307 11 19a1 1 0 1 0 2 0c0-1.692 2.046-2.54 3.243-1.343a1 1 0 1 0 1.414-1.414C16.46 15.046 17.307 13 19 13a1 1 0 1 0 0-2c-1.692 0-2.54-2.046-1.343-3.243a1 1 0 0 0-1.414-1.414C15.046 7.54 13 6.693 13 5a1 1 0 0 0-1-1zm-2.992.777a3 3 0 0 1 5.984 0 3 3 0 0 1 4.23 4.231 3 3 0 0 1 .001 5.984 3 3 0 0 1-4.231 4.23 3 3 0 0 1-5.984 0 3 3 0 0 1-4.231-4.23 3 3 0 0 1 0-5.984 3 3 0 0 1 4.231-4.231z" />
          <path d="M12 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm-2.828-.828a4 4 0 1 1 5.656 5.656 4 4 0 0 1-5.656-5.656z" />
        </g>
      )
    case 'extensions':
      return <path d="M5.25 2.25h3.5v2h2.75v3.5h-2v3.5H6v-2H2.5v-3.5h2.75v-3.5Z" />
    case 'indexing':
      return (
        <>
          <ellipse cx="7" cy="3.25" rx="4.25" ry="1.75" />
          <path d="M2.75 3.25v3.25c0 .97 1.9 1.75 4.25 1.75s4.25-.78 4.25-1.75V3.25M2.75 6.5v3.25c0 .97 1.9 1.75 4.25 1.75s4.25-.78 4.25-1.75V6.5" />
        </>
      )
    case 'snippets':
      return (
        <>
          <rect x="3" y="2.25" width="8" height="9.5" rx="1.25" />
          <path d="M5 5h4M5 7h4M5 9h2.5" />
        </>
      )
    case 'notes':
      return (
        <>
          <path d="M3.25 2.25h7.5v7L8 12H3.25z" />
          <path d="M8 12V9.25h2.75M5 5h4M5 7h3" />
        </>
      )
    case 'emoji':
      return (
        <>
          <circle cx="7" cy="7" r="4.75" />
          <path d="M5.25 5.75h.01M8.75 5.75h.01M5.1 8.2c.45.75 1.1 1.15 1.9 1.15s1.45-.4 1.9-1.15" />
        </>
      )
    case 'clipboard':
      return (
        <>
          <rect x="3" y="3.5" width="8" height="8.5" rx="1.25" />
          <path d="M5.25 4V2.75h3.5V4M5 7h4M5 9.5h3" />
        </>
      )
    case 'key':
      // Password-generation art: full-bleed field, thinner strokes, dots on
      // pixel centers, check swung wider for breathing room. No card tile.
      return (
        <>
          <rect x="0.4" y="3" width="13.2" height="8" rx="1.8" strokeWidth="0.8" />
          <path d="M2 7H2.02M4.7 7H4.72M7.4 7H7.42" strokeWidth="1.2" />
          <path d="M9.1 6.55 10 7.45 11.7 5.75" strokeWidth="0.95" />
        </>
      )
    case 'moon':
      return <path d="M11.55 8.45A4.65 4.65 0 1 1 5.95 2.85 3.55 3.55 0 0 0 11.55 8.45Z" />
    case 'display':
      return (
        <>
          <rect x="2" y="2.75" width="10" height="7" rx="1.25" />
          <path d="M5.25 12h3.5M7 9.75V12M5.25 5.25h3.5M7 3.9v2.7" />
        </>
      )
    case 'display-sleep':
      return (
        <>
          <rect x="2" y="3" width="10" height="6.75" rx="1.25" />
          <path d="M5.25 12h3.5M7 9.75V12M5.4 5.6h3.2L5.4 8.1h3.2" />
        </>
      )
    case 'volume':
      return (
        <>
          <path d="M2.5 5.25h2.25L7.5 3v8L4.75 8.75H2.5z" />
          <path d="M9.25 5.25c.5.45.75 1.05.75 1.75s-.25 1.3-.75 1.75M10.75 3.75c.9.85 1.35 1.93 1.35 3.25s-.45 2.4-1.35 3.25" />
        </>
      )
    case 'volume-muted':
      return (
        <>
          <path d="M2.5 5.25h2.25L7.5 3v8L4.75 8.75H2.5z" />
          <path d="m9.25 5.25 2.5 2.5M11.75 5.25l-2.5 2.5" />
        </>
      )
    case 'desktop':
      return (
        <>
          <rect x="2" y="2.75" width="10" height="7" rx="1.25" />
          <path d="M4 5h2M4 7h2M8 5h2M8 7h2M5.25 12h3.5M7 9.75V12" />
        </>
      )
    case 'dock':
      return (
        <>
          <rect x="2.25" y="8.25" width="9.5" height="3" rx="1.25" />
          <rect x="3.25" y="3" width="2" height="3.25" rx=".6" />
          <rect x="6" y="3" width="2" height="3.25" rx=".6" />
          <rect x="8.75" y="3" width="2" height="3.25" rx=".6" />
        </>
      )
    case 'menu-bar':
      return (
        <>
          <rect x="2" y="3" width="10" height="8" rx="1.25" />
          <path d="M2 5.25h10M4 7.25h2M8 7.25h2M4 9.25h4" />
        </>
      )
    case 'finder':
      return (
        <>
          <rect x="2.5" y="2.25" width="9" height="9.5" rx="1.5" />
          <path d="M7 2.25v9.5M4.75 5.4h.01M9.25 5.4h.01M4.75 8.8c.6.45 1.35.7 2.25.7s1.65-.25 2.25-.7" />
        </>
      )
    case 'awake':
      return (
        <>
          <circle cx="7" cy="7" r="2.5" />
          <path d="M7 1.75v1.2M7 11.05v1.2M1.75 7h1.2M11.05 7h1.2M3.3 3.3l.85.85M9.85 9.85l.85.85M3.3 10.7l.85-.85M9.85 4.15l.85-.85" />
        </>
      )
    case 'sleep':
      return (
        <path d="M3.25 8.75h2.9L3.25 11h2.9M7.25 4.75h3.5L7.25 7.5h3.5M4.5 2.25h4.75L4.5 5.75h4.75" />
      )
    case 'bluetooth':
      return (
        <>
          <path d="M7 7V1.75l2.92 2.63Z" />
          <path d="m9.92 9.63L7 7v5.25Z" />
          <path d="M7 7 5.25 8.75M7 7 5.25 5.25" />
        </>
      )
    case 'wifi':
      return (
        <>
          <path d="M2.25 5.25a7.25 7.25 0 0 1 9.5 0M4.25 7.25a4.25 4.25 0 0 1 5.5 0M6.1 9.15a1.4 1.4 0 0 1 1.8 0" />
          <circle cx="7" cy="11" r=".35" fill="currentColor" stroke="none" />
        </>
      )
    case 'network':
      return (
        <>
          <circle cx="7" cy="7" r="4.75" />
          <path d="M2.75 7h8.5M7 2.25c1.1 1.25 1.65 2.85 1.65 4.75S8.1 10.5 7 11.75M7 2.25C5.9 3.5 5.35 5.1 5.35 7S5.9 10.5 7 11.75" />
        </>
      )
    case 'ip':
      return (
        <>
          <path d="M7 12s4-3.35 4-6.25a4 4 0 0 0-8 0C3 8.65 7 12 7 12Z" />
          <circle cx="7" cy="5.75" r="1.25" />
        </>
      )
    case 'dns':
      return (
        <>
          <rect x="2.5" y="3" width="9" height="3.5" rx="1" />
          <rect x="2.5" y="7.5" width="9" height="3.5" rx="1" />
          <path d="M4.5 4.75h.01M4.5 9.25h.01M7 6.5v1" />
        </>
      )
    case 'vpn':
      return (
        <>
          <path d="M7 2.25 11 4v2.65c0 2.45-1.35 4.1-4 5.1-2.65-1-4-2.65-4-5.1V4z" />
          <path d="M5.3 7.1 6.5 8.3l2.4-2.6" />
        </>
      )
    case 'trash':
      return (
        <>
          <path d="M3.25 4h7.5M5.25 4V2.75h3.5V4M4.25 5.5l.45 5.5h4.6l.45-5.5M6 6.75v2.75M8 6.75v2.75" />
        </>
      )
    case 'lock':
      return (
        <>
          <rect x="3.25" y="6" width="7.5" height="5.25" rx="1.25" />
          <path d="M5 6V4.75a2 2 0 1 1 4 0V6" />
        </>
      )
    case 'folder-downloads':
      return (
        <>
          <path d="M2 4.5h3l1 1h6v5.75a.75.75 0 0 1-.75.75h-8.5a.75.75 0 0 1-.75-.75V4.5Z" />
          <path d="M7 6.75v3M5.75 8.75 7 10l1.25-1.25" />
        </>
      )
    case 'applications':
      return (
        <>
          <rect x="2" y="2" width="4" height="4" rx="1" />
          <rect x="8" y="2" width="4" height="4" rx="1" />
          <rect x="2" y="8" width="4" height="4" rx="1" />
          <rect x="8" y="8" width="4" height="4" rx="1" />
        </>
      )
    case 'library':
      return (
        <>
          <path d="M2.5 11.5h9M3.5 9.75v-4.5M6 9.75v-4.5M8.5 9.75v-4.5M11 9.75v-4.5M2.25 5.25 7 2.5l4.75 2.75z" />
        </>
      )
    case 'copy-path':
      return (
        <>
          <rect x="4.75" y="3.25" width="6.25" height="7.25" rx="1.25" />
          <path d="M3 5.25v5.5h5M5.25 7h3.5M5.25 8.75h2.5" />
        </>
      )
    case 'quit':
      return (
        <>
          <path d="M7 2.25v4.25" />
          <path d="M4.4 3.9a4.5 4.5 0 1 0 5.2 0" />
        </>
      )
    case 'macos':
      return (
        <>
          <rect x="2.25" y="2.75" width="9.5" height="7.25" rx="1.25" />
          <path d="M5.25 12h3.5M7 10v2M4.5 5.25h.01M6.25 5.25h.01M8 5.25h.01" />
        </>
      )
    case 'cpu':
      return (
        <>
          <rect x="4" y="4" width="6" height="6" rx="1" />
          <path d="M5.75 1.75v1.5M8.25 1.75v1.5M5.75 10.75v1.5M8.25 10.75v1.5M1.75 5.75h1.5M1.75 8.25h1.5M10.75 5.75h1.5M10.75 8.25h1.5M6 6h2v2H6z" />
        </>
      )
    case 'memory':
      return (
        <>
          <rect x="2.5" y="4" width="9" height="6" rx="1.25" />
          <path d="M4 2.5v1.5M6 2.5v1.5M8 2.5v1.5M10 2.5v1.5M4 10v1.5M6 10v1.5M8 10v1.5M10 10v1.5M4.75 6.25h4.5M4.75 8h3" />
        </>
      )
    case 'disk':
      return (
        <>
          <ellipse cx="7" cy="4" rx="4.5" ry="1.75" />
          <path d="M2.5 4v5.75C2.5 10.72 4.5 12 7 12s4.5-1.28 4.5-2.25V4M2.5 7c0 .97 2 1.75 4.5 1.75s4.5-.78 4.5-1.75" />
        </>
      )
    case 'battery':
      return (
        <>
          <rect x="2" y="4.5" width="8.5" height="5" rx="1.25" />
          <path d="M10.5 6h1.25v2H10.5M5.75 5.75 4.9 7.2h1.35l-.85 1.55" />
        </>
      )
    case 'ports':
      return (
        <>
          <path d="M4 7H2.75A1.75 1.75 0 0 1 1 5.25V4.5A1.75 1.75 0 0 1 2.75 2.75H4M10 7h1.25A1.75 1.75 0 0 0 13 5.25V4.5a1.75 1.75 0 0 0-1.75-1.75H10M4 4.9h6M4 9.1h6M7 4.9v4.2M3 11.25h8" />
        </>
      )
    case 'git':
      return (
        <>
          <path d="M7 1.75 12.25 7 7 12.25 1.75 7z" />
          <path d="M5.5 5.5 7 7m0 0 1.5 1.5M7 7V3.75M7 7h3.25" />
          <circle cx="7" cy="7" r=".35" fill="currentColor" stroke="none" />
        </>
      )
    case 'brew':
      return (
        <>
          <path d="M4.25 5.25h5.5l-.55 5.5h-4.4zM4.75 5.25 4.25 3h5.5l-.5 2.25M9.75 6.25h1a1.25 1.25 0 0 1 0 2.5H9.5" />
          <path d="M5.5 3V2M7 3V2M8.5 3V2" />
        </>
      )
    case 'terminal':
    default:
      return (
        <>
          <rect x="1.75" y="2.25" width="10.5" height="9.5" rx="1.5" />
          <path d="m4 5 2 2-2 2M7.5 9h2.5" />
        </>
      )
  }
}

const resolvedAssetIconCache = new Map<string, string | null>()
const pendingAssetIconCache = new Map<string, Promise<string | null>>()

function loadAssetIcon(kind: IconAssetKind, path: string): Promise<string | null> {
  const key = `${kind}:${path}`
  if (resolvedAssetIconCache.has(key)) {
    return Promise.resolve(resolvedAssetIconCache.get(key) ?? null)
  }
  const pending = pendingAssetIconCache.get(key)
  if (pending) return pending

  const request = window.tezbar
    .getAssetIconDataUrl(kind, path)
    .then((icon) => {
      resolvedAssetIconCache.set(key, icon)
      return icon
    })
    .catch(() => {
      resolvedAssetIconCache.set(key, null)
      return null
    })
    .finally(() => pendingAssetIconCache.delete(key))
  pendingAssetIconCache.set(key, request)
  return request
}

function ListItemIcon({
  kind,
  iconDataUrl,
  assetKind,
  assetPath,
  commandIcon,
}: {
  kind: ListItemIconKind
  iconDataUrl?: string
  assetKind?: IconAssetKind
  assetPath?: string
  commandIcon?: CommandIconKind
}): ReactNode {
  const iconContainerRef = useRef<HTMLSpanElement>(null)
  const [loadedAssetIcon, setLoadedAssetIcon] = useState<{
    key: string
    icon: string
  } | null>(null)
  const assetKey = assetKind && assetPath ? `${assetKind}:${assetPath}` : null
  const resolvedIconDataUrl =
    (loadedAssetIcon && loadedAssetIcon.key === assetKey ? loadedAssetIcon.icon : undefined) ??
    iconDataUrl

  useEffect(() => {
    if (!assetKind || !assetPath) return

    let cancelled = false
    const load = (): void => {
      void loadAssetIcon(assetKind, assetPath).then((icon) => {
        if (!cancelled && icon) setLoadedAssetIcon({ key: `${assetKind}:${assetPath}`, icon })
      })
    }
    const element = iconContainerRef.current
    if (!element || typeof IntersectionObserver === 'undefined') {
      load()
      return () => {
        cancelled = true
      }
    }

    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return
        observer.disconnect()
        load()
      },
      { rootMargin: '80px' }
    )
    observer.observe(element)
    return () => {
      cancelled = true
      observer.disconnect()
    }
  }, [assetKind, assetPath])

  const tone = commandIcon
    ? commandIconTone(commandIcon)
    : kind === 'directory'
      ? 'border-sky-400/20 bg-sky-400/10 text-sky-300'
      : kind === 'application' || kind === 'applications'
        ? 'border-violet-400/20 bg-violet-400/10 text-violet-300'
        : kind === 'extensions' || kind === 'store'
          ? 'border-amber-400/20 bg-amber-400/10 text-amber-300'
          : kind === 'clipboard'
            ? 'border-emerald-400/20 bg-emerald-400/10 text-emerald-300'
            : 'border-white/10 bg-white/[0.04] text-ink-3'

  const glyph = (() => {
    if (commandIcon) {
      return <CommandIconGlyph kind={commandIcon} />
    }
    if (kind === 'directory') {
      return <path d="M2 4.5h3l1 1h6v5.75a.75.75 0 0 1-.75.75h-8.5a.75.75 0 0 1-.75-.75V4.5Z" />
    }
    if (kind === 'application' || kind === 'applications') {
      return (
        <>
          <rect x="2" y="2" width="4" height="4" rx="1" />
          <rect x="8" y="2" width="4" height="4" rx="1" />
          <rect x="2" y="8" width="4" height="4" rx="1" />
          <rect x="8" y="8" width="4" height="4" rx="1" />
        </>
      )
    }
    if (kind === 'extensions' || kind === 'store') {
      return <path d="M5.25 2.25h3.5v2h2.75v3.5h-2v3.5H6v-2H2.5v-3.5h2.75v-3.5Z" />
    }
    if (kind === 'clipboard') {
      return (
        <>
          <rect x="3" y="3.5" width="8" height="8.5" rx="1.25" />
          <path d="M5.25 4V2.75h3.5V4M5 7h4M5 9.5h3" />
        </>
      )
    }
    if (kind === 'quick-notes' || kind === 'snippets') {
      return (
        <>
          <path d="M3 2h8v10H3zM5 5h4M5 7.5h4" />
          <path d="M8 12v-2h3" />
        </>
      )
    }
    if (kind === 'quick-links') {
      return (
        <>
          <path d="M5.75 8.25 8.25 5.75" />
          <path d="M4.75 9.75H4a2.25 2.25 0 0 1 0-4.5h2M9.25 4.25H10a2.25 2.25 0 0 1 0 4.5H8" />
        </>
      )
    }
    if (kind === 'native-command' || kind === 'commands' || kind === 'mac-cli') {
      return (
        <>
          <rect x="1.75" y="2.25" width="10.5" height="9.5" rx="1.5" />
          <path d="m4 5 2 2-2 2M7.5 9h2.5" />
        </>
      )
    }
    return (
      <>
        <path d="M3 1.75h5l3 3V12H3z" />
        <path d="M8 1.75v3h3" />
      </>
    )
  })()

  return (
    <span
      ref={iconContainerRef}
      aria-hidden
      className={cx(
        'relative grid h-7 w-7 shrink-0 place-items-center',
        resolvedIconDataUrl
          ? 'overflow-visible border border-transparent bg-transparent'
          : cx('overflow-hidden rounded-[7px] border', tone)
      )}
    >
      {!resolvedIconDataUrl ? (
        <svg
          width="16"
          height="16"
          viewBox="0 0 14 14"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.15"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          {glyph}
        </svg>
      ) : null}
      {resolvedIconDataUrl ? (
        <img
          src={resolvedIconDataUrl}
          alt=""
          className="absolute inset-0 h-full w-full object-contain"
          draggable={false}
          onError={(event) => {
            event.currentTarget.style.display = 'none'
          }}
        />
      ) : null}
    </span>
  )
}

function pathCompletionSectionLabel(section: PathCompletionItem['section']): string | null {
  if (section === 'recommended') return 'Recommended'
  if (section === 'default') return 'Default'
  if (section === 'applications') return 'All Applications'
  return null
}

function completionIconAsset(item: PathCompletionItem): {
  kind: IconAssetKind
  path: string
} | null {
  if (item.kind === 'application') {
    const appPath = item.appPath ?? (item.applicationAction === 'open' ? item.path : undefined)
    return appPath ? { kind: 'application', path: appPath } : null
  }
  if (item.kind === 'file' && item.path) return { kind: 'file', path: item.path }
  return null
}

function searchResultIconAsset(item: SearchResult): {
  kind: IconAssetKind
  path: string
} | null {
  if (item.category === 'applications' && item.subtitle.endsWith('.app')) {
    return { kind: 'application', path: item.subtitle }
  }
  if (
    (item.category === 'files' || item.category === 'knowledge') &&
    item.action.type === 'open-file'
  ) {
    return { kind: 'file', path: item.action.path }
  }
  if (
    item.category === 'extensions' &&
    item.action.type === 'run-extension-command' &&
    item.action.iconPath
  ) {
    return { kind: 'extension', path: item.action.iconPath }
  }
  return null
}

export default function CommandBar({
  initialValue = '',
  initialSelectedChatId = null,
  onOpenAiChat,
  onOpenSettings,
  onOpenExtensionsSettings,
  onConfigureAi,
  onOpenExtensions,
  onOpenExtensionRuntime,
  onOpenPortsPage,
  onOpenSystemMonitor,
  onOpenIndexingPage,
  onOpenClipboardPage,
  onOpenSnippetsPage,
  onOpenNotesPage,
  onOpenEmojiPicker,
  onOpenTerminal,
}: {
  initialValue?: string
  initialSelectedChatId?: string | null
  onOpenAiChat: (boot: AiChatBoot) => void
  onOpenSettings: () => void
  onOpenExtensionsSettings: () => void
  onConfigureAi: () => void
  onOpenExtensions: () => void
  onOpenExtensionRuntime: (initial: ExtensionRuntimeViewPayload) => void
  onOpenPortsPage: (opts?: { tab?: 'listen' | 'named' }) => void
  onOpenSystemMonitor: () => void
  onOpenIndexingPage: () => void
  onOpenClipboardPage: () => void
  onOpenSnippetsPage: () => void
  onOpenNotesPage: (opts?: { createdAt?: number }) => void
  onOpenEmojiPicker: () => void
  onOpenTerminal: (
    initialCommand?: string,
    workingDirectory?: string,
    sessionId?: string,
    defaults?: TerminalDefaults
  ) => void
}): JSX.Element {
  const normalizedInitialValue = initialValue.startsWith('>') ? initialValue.slice(1) : initialValue
  const [value, setValue] = useState(normalizedInitialValue)
  const [prevInitialValue, setPrevInitialValue] = useState(initialValue)
  const [error, setError] = useState<string | null>(null)
  const [streamText, setStreamText] = useState('')
  const [streamError, setStreamError] = useState<string | null>(null)
  const [isStreaming, setIsStreaming] = useState(false)
  const [cfg, setCfg] = useState<LlmConfigRecord>({})
  const [modelMenuOpen, setModelMenuOpen] = useState(false)
  const [emptyAnswer, setEmptyAnswer] = useState(false)
  const [pathCompletions, setPathCompletions] = useState<PathCompletionItem[]>([])
  const [recentExtensionCommands, setRecentExtensionCommands] = useState<string[]>([])
  const [pinnedCommands, setPinnedCommands] = useState<PinnedCommand[]>(readPinnedCommands)
  const [pinnedCommandTooltip, setPinnedCommandTooltip] = useState<PinnedCommandTooltip | null>(
    null
  )
  const [chatHistory, setChatHistory] = useState<ChatSessionSummary[]>([])
  const [draggingPinId, setDraggingPinId] = useState<string | null>(null)
  const [draggingSearchResultId, setDraggingSearchResultId] = useState<string | null>(null)
  const [pinDropIndex, setPinDropIndex] = useState<number | null>(null)
  const [pinUnpinDropActive, setPinUnpinDropActive] = useState(false)
  const [selectedSuggestion, setSelectedSuggestion] = useState(0)
  const [followSuggestionSelection, setFollowSuggestionSelection] = useState(false)
  const [searchResults, setSearchResults] = useState<SearchResult[]>(readInitialSearchResults)
  const [deepSearchLoadingQuery, setDeepSearchLoadingQuery] = useState<string | null>(null)
  const [cachedSearchCandidates] = useState<SearchResult[]>(readCachedSearchCandidates)
  const [initialSearchCandidates] = useState<SearchResult[]>(() =>
    mergeSearchCandidates(searchResults, cachedSearchCandidates)
  )
  const homeSearchResultsRef = useRef<SearchResult[]>(searchResults)
  const searchCandidatesRef = useRef<SearchResult[]>(initialSearchCandidates)
  const queryResultsCacheRef = useRef<Map<string, { items: SearchResult[]; cachedAt: number }>>(
    new Map()
  )
  const [selectedSearch, setSelectedSearch] = useState(() =>
    initialValue.startsWith('>') ||
    initialValue.startsWith(' ') ||
    initialValue.endsWith('  ') ||
    parseSearchQuery(initialValue).mode === 'deep'
      ? -1
      : 0
  )
  const [followSearchSelection, setFollowSearchSelection] = useState(false)
  const [searchResultNavigationActive, setSearchResultNavigationActive] = useState(false)
  const [actionMsg, setActionMsg] = useState<string | null>(null)
  const [actionMsgCard, setActionMsgCard] = useState<{
    kind: string
    iconKind?: CommandIconKind
  } | null>(null)
  const actionMsgTimeoutRef = useRef<NodeJS.Timeout | null>(null)

  const showActionMsg = (
    msg: string | null,
    card: { kind: string; iconKind?: CommandIconKind } | null = null
  ): void => {
    if (actionMsgTimeoutRef.current) {
      clearTimeout(actionMsgTimeoutRef.current)
      actionMsgTimeoutRef.current = null
    }
    setActionMsg(msg)
    setActionMsgCard(card)
    if (msg) {
      // Generated passwords stay up longest (read + transcribe the secret);
      // info cards outlive plain status blurbs so output can be scanned.
      const ttl = card?.kind === 'password' ? 9000 : card ? 7000 : 4000
      actionMsgTimeoutRef.current = setTimeout(() => {
        setActionMsg(null)
        actionMsgTimeoutRef.current = null
      }, ttl)
    }
  }
  const [terminalMode, setTerminalMode] = useState(() => initialValue.startsWith('>'))
  // Space prefix = AI mode in the launcher; the full chat UI lives on the
  // dedicated AI Chat surface (see App.tsx + AgentChatView).
  // Trigger AI mode if input starts with a space, or if it ends with exactly two spaces.
  const parsedSearchQuery = parseSearchQuery(value)
  // Terminal mode owns the input completely. A slash, backtick, or deep-search
  // prefix typed after `>` is shell text, not a launcher mode switch.
  const isDeepSearchMode = !terminalMode && parsedSearchQuery.mode === 'deep'
  const isAiMode =
    !terminalMode && !isDeepSearchMode && (value.startsWith(' ') || value.endsWith('  '))
  const deepSearchQuery = isDeepSearchMode ? parsedSearchQuery.query : ''
  const normalizedSearchValue = searchRequestInput(value)
  const agentTask = isAiMode ? value.trim() : ''

  const [pendingAction, setPendingAction] = useState<{
    extensionId: string
    commandName: string
    title: string
    subtitle?: string
    iconDataUrl?: string
    iconPath?: string
    commandArgumentDefinitions: PendingExtensionArgument[]
  } | null>(null)
  const [argumentValues, setArgumentValues] = useState<Record<string, string>>({})
  const pendingInlineArgument =
    pendingAction?.commandArgumentDefinitions.length === 1 &&
    pendingAction.commandArgumentDefinitions[0]?.type !== 'dropdown'
      ? pendingAction.commandArgumentDefinitions[0]
      : null
  const pendingActionUsesAi = pendingAction ? isAiPoweredExtensionAction(pendingAction) : false
  const actionProvider = (cfg.taskProviderOverrides?.action ?? cfg.provider ?? 'ollama') as ProviderId
  const actionModel =
    cfg.taskModelOverrides?.action ??
    cfg.providerSelectedModels?.[actionProvider] ??
    (cfg.provider === actionProvider ? cfg.model : undefined) ??
    recommendedModel(actionProvider)
  const actionModelConfig: LlmConfigRecord = {
    ...cfg,
    provider: actionProvider,
    model: actionModel,
    providerSelectedModels: {
      ...cfg.providerSelectedModels,
      [actionProvider]: actionModel,
    },
  }
  const [killPortMode, setKillPortMode] = useState(false)
  const [killPortQuery, setKillPortQuery] = useState('')
  const [killPortValue, setKillPortValue] = useState('')
  const [killPortArgumentDismissed, setKillPortArgumentDismissed] = useState(false)
  const [terminalPrompt, setTerminalPrompt] = useState('')
  const [terminalSessions, setTerminalSessions] = useState<TerminalSessionSummary[]>([])
  const [pinnedTerminalSessionIds, setPinnedTerminalSessionIds] = useState<string[]>([])
  const [terminalSettingsOpen, setTerminalSettingsOpen] = useState(false)
  const [terminalSettingsDraft, setTerminalSettingsDraft] = useState<TerminalDefaults>(() =>
    readTerminalDefaults()
  )
  const terminalWorkingDirectoryRef = useRef<string | undefined>()
  const argInputRefs = useRef<Array<HTMLInputElement | HTMLSelectElement | null>>([])
  const gotAnyTokenRef = useRef(false)
  const pendingOpenRef = useRef(false)
  const modelSelectionSaveRef = useRef<Promise<void> | null>(null)
  const modelMenuOpenRef = useRef(false)
  const valueRef = useRef(value)
  const killPortModeRef = useRef(killPortMode)
  const killPortValueRef = useRef(killPortValue)
  const terminalModeRef = useRef(terminalMode)
  // A terminal row is only eligible for Enter after keyboard navigation has
  // explicitly selected the session. Hovering a row (or a stale search index)
  // must not turn a fresh `>` launch into an implicit restore of row 0.
  const terminalSessionSelectionActiveRef = useRef(false)
  const terminalSettingsOpenRef = useRef(terminalSettingsOpen)
  const launcherQueryHistoryRef = useRef<string[]>([])
  const lastSearchRequestId = useRef(0)
  const draggingPinIdRef = useRef<string | null>(null)
  const pinDropIndexRef = useRef<number | null>(null)
  const pinRailRef = useRef<HTMLDivElement | null>(null)
  const pinPointerActiveRef = useRef(false)
  const pinPointerStartYRef = useRef(0)
  const pinUnpinDropActiveRef = useRef(false)
  const suppressNextPinClickRef = useRef(false)
  const resultPointerActiveRef = useRef(false)
  const resultPointerDraggedRef = useRef(false)
  const resultPointerStartRef = useRef({ x: 0, y: 0 })
  const draggingSearchResultRef = useRef<SearchResult | null>(null)
  const suppressNextSearchResultClickRef = useRef(false)
  const quickLookPendingRef = useRef(false)
  const searchResultsNavigationRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    valueRef.current = value
  }, [value])

  useEffect(() => {
    killPortModeRef.current = killPortMode
  }, [killPortMode])

  useEffect(() => {
    killPortValueRef.current = killPortValue
  }, [killPortValue])

  useEffect(() => {
    const handleKillPortEscape = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape' || !killPortModeRef.current) return

      event.preventDefault()
      event.stopImmediatePropagation()

      if (killPortValueRef.current) {
        killPortValueRef.current = ''
        setKillPortValue('')
      } else {
        setKillPortArgumentDismissed(true)
        killPortModeRef.current = false
        setKillPortMode(false)
      }

      showActionMsg(null)
      requestAnimationFrame(() => document.getElementById('command-input')?.focus())
    }

    window.addEventListener('keydown', handleKillPortEscape, true)
    return () => window.removeEventListener('keydown', handleKillPortEscape, true)
  }, [])

  useEffect(() => {
    terminalModeRef.current = terminalMode
  }, [terminalMode])

  useEffect(() => {
    terminalSettingsOpenRef.current = terminalSettingsOpen
  }, [terminalSettingsOpen])

  if (initialValue !== prevInitialValue) {
    setPrevInitialValue(initialValue)
    setTerminalMode(initialValue.startsWith('>'))
    terminalSessionSelectionActiveRef.current = false
    setValue(normalizedInitialValue)
    setSelectedSearch(
      initialValue.startsWith('>') ||
        initialValue.startsWith(' ') ||
        initialValue.endsWith('  ') ||
        parseSearchQuery(initialValue).mode === 'deep'
        ? -1
        : 0
    )
  }

  useEffect(() => {
    if (!terminalMode) {
      setTerminalPrompt('')
      return
    }
    void window.tezbar.getTerminalPromptInfo().then((info: TerminalPromptInfo) => {
      if (info) {
        const directory = compactTerminalPath(terminalWorkingDirectoryRef.current || info.dir)
        const prompt = `${info.user}@${info.host} ${directory} %`
        setTerminalPrompt(prompt)
      }
    })
  }, [terminalMode])

  useEffect(() => {
    if (!terminalMode) {
      setTerminalSessions([])
      return
    }
    let cancelled = false
    let interval: number | null = null
    const load = async (): Promise<void> => {
      try {
        const sessions = await window.tezbar.terminalList()
        if (!cancelled) setTerminalSessions(sessions)
      } catch {
        if (!cancelled) setTerminalSessions([])
      }
    }
    const startPolling = (): void => {
      if (interval !== null) return
      interval = window.setInterval(() => {
        void load()
      }, 2500)
    }
    const stopPolling = (): void => {
      if (interval !== null) window.clearInterval(interval)
      interval = null
    }
    void load()
    startPolling()
    // Pause when the launcher is hidden — there's no status dot to update if
    // nobody can see it.
    const onVisibility = (): void => {
      if (document.hidden) stopPolling()
      else {
        void load()
        startPolling()
      }
    }
    document.addEventListener('visibilitychange', onVisibility)
    return () => {
      cancelled = true
      stopPolling()
      document.removeEventListener('visibilitychange', onVisibility)
    }
  }, [terminalMode])

  useEffect(() => {
    const launcherQueryHistory = readLauncherQueryHistory()
    launcherQueryHistoryRef.current = launcherQueryHistory
    writeLauncherQueryHistory(launcherQueryHistory)
    setRecentExtensionCommands(readRecentExtensionCommands())
    setPinnedTerminalSessionIds(readPinnedTerminalSessionIds())
    void window.tezbar.chatList(40).then(setChatHistory)
  }, [])

  useEffect(() => {
    if (!initialSelectedChatId || !isAiMode || chatHistory.length === 0) return
    const index = chatHistory.findIndex((chat) => chat.id === initialSelectedChatId)
    if (index >= 0) setSelectedSearch(index)
  }, [chatHistory, initialSelectedChatId, isAiMode])

  useEffect(() => {
    const offToken = window.tezbar.onStreamToken((t) => {
      gotAnyTokenRef.current = true
      setStreamText((s) => s + t)
    })
    const offDone = window.tezbar.onStreamDone(() => {
      setIsStreaming(false)
      setStreamError(null)
      setEmptyAnswer(!gotAnyTokenRef.current)
    })
    const offErr = window.tezbar.onStreamError((m) => {
      setIsStreaming(false)
      setEmptyAnswer(false)
      setStreamError(m)
    })
    return () => {
      offToken()
      offDone()
      offErr()
    }
  }, [])

  useEffect(() => {
    void window.tezbar.getLlmConfig().then((c) => setCfg(c as LlmConfigRecord))
  }, [])

  useEffect(() => {
    const onQuickNoteShortcut = (): void => {
      const currentValue = valueRef.current
      if (currentValue.startsWith(' ')) {
        onOpenAiChat({ kind: 'newChat' })
        showActionMsg('Opening new chat')
        return
      }

      const text = currentValue.trim()
      if (!text) {
        showActionMsg('Type text in the command bar, then press Cmd+N to save a note')
        return
      }
      void window.tezbar
        .appendQuickNote(text)
        .then((entry) => {
          if (!entry) {
            showActionMsg('Nothing to save')
            return
          }
          void window.tezbar.searchAll(valueRef.current).then((items) => {
            const deepSearchActive = parseSearchQuery(valueRef.current).mode === 'deep'
            setSearchResults(items)
            setSearchResultNavigationActive(false)
            setSelectedSearch(deepSearchActive ? -1 : 0)
            setFollowSearchSelection(!deepSearchActive)
          })
          showActionMsg('Saved to Quick Notes')
        })
        .catch(() => {
          showActionMsg('Could not save quick note')
        })
    }
    window.addEventListener(RAYMES_QUICK_NOTE_SHORTCUT_EVENT, onQuickNoteShortcut)
    return () => window.removeEventListener(RAYMES_QUICK_NOTE_SHORTCUT_EVENT, onQuickNoteShortcut)
  }, [onOpenAiChat])

  // Hold-to-Speak pipeline: captures mic audio via MediaRecorder, resamples
  // to 16 kHz mono WAV in the renderer, and hands the bytes to the main
  // process for local transcription (whisper-cli / moonshine). See
  // `useHoldToSpeak` for the full rationale and the reason we no longer
  // use `webkitSpeechRecognition`.
  const holdToSpeak = useHoldToSpeak({
    onMessage: (message) => showActionMsg(message),
    onTranscript: (text) => {
      const cleaned = text.trim()
      if (!cleaned) {
        showActionMsg('Nothing was transcribed. Try speaking louder or for longer.')
        return
      }
      setValue((prev) => {
        const isAiModeActive = prev.startsWith(' ') || prev.endsWith('  ')
        if (!prev.trim()) {
          // If the buffer was literally just spaces (prompting AI mode),
          // preserve those spaces so the transcription result is treated as an AI prompt.
          // Note: if it started with at least one space, we keep it as an AI prompt.
          return isAiModeActive ? ` ${cleaned}` : cleaned
        }
        if (isAiModeActive) {
          // If already in AI mode (e.g. ends in two spaces), append.
          return `${prev}${cleaned}`
        }
        return `${prev} ${cleaned}`
      })
    },
  })

  const slashQuery = value.trimStart()
  const isSlashInput = !terminalMode && slashQuery.startsWith('/')
  const isApplicationInput = !terminalMode && slashQuery.startsWith('`')
  const isCompletionInput = !terminalMode && (isSlashInput || isApplicationInput)

  const chatHistoryQuery = agentTask.trim().toLowerCase()
  const filteredChatHistory = useMemo(() => {
    if (!chatHistoryQuery) return chatHistory
    const terms = chatHistoryQuery.split(/\s+/).filter(Boolean)
    return chatHistory.filter((chat) => {
      const haystack = `${chat.title} ${chat.preview}`.toLowerCase()
      return terms.every((term) => haystack.includes(term))
    })
  }, [chatHistory, chatHistoryQuery])
  const showChatHistory = isAiMode && filteredChatHistory.length > 0

  const filteredTerminalSessions = useMemo(() => {
    if (!terminalMode) return []
    const q = value.trim().toLowerCase()
    if (!q) return terminalSessions
    const terms = q.split(/\s+/).filter(Boolean)
    return terminalSessions.filter((session) => {
      const haystack =
        `${session.name} ${session.cwd} ${session.lastCommand ?? session.initialCommand ?? ''}`.toLowerCase()
      return terms.every((term) => haystack.includes(term))
    })
  }, [terminalMode, terminalSessions, value])
  const orderedTerminalSessions = useMemo(() => {
    const pinned = new Set(pinnedTerminalSessionIds)
    return [...filteredTerminalSessions].sort((a, b) => {
      const pinnedDelta = Number(pinned.has(b.sessionId)) - Number(pinned.has(a.sessionId))
      return pinnedDelta || b.updatedAt - a.updatedAt
    })
  }, [filteredTerminalSessions, pinnedTerminalSessionIds])
  const terminalSessionCount = orderedTerminalSessions.length
  const showTerminalSessions = terminalMode

  // Live calculator: we evaluate on every keystroke in the renderer so
  // there's no IPC latency. Only when the buffer is not a slash command —
  // `/providers` shouldn't trigger math.js.
  const mathCalc: CalcResult | null = useMemo(() => {
    if (isCompletionInput) return null
    const t = value.trim()
    if (t && parseCurrencyQuery(t, getPreferredDefaultTarget())) {
      return null
    }
    return evaluateExpression(value)
  }, [isCompletionInput, value])
  const currencyCalc = useCurrencyConversion(value, isCompletionInput)
  const calc = currencyCalc ?? mathCalc

  const calcResultRow: SearchResult | null = useMemo(() => {
    if (!calc) return null
    if (currencyCalc) {
      const subtitle = `${currencyCalc.amountFormatted} → ${currencyCalc.to}`
      return {
        id: `currency:${currencyCalc.from}-${currencyCalc.to}-${currencyCalc.amount}`,
        title: currencyCalc.formatted,
        subtitle,
        category: 'calculator',
        score: 10_000,
        action: { type: 'copy-text', text: currencyCalc.clipboard },
      }
    }
    return {
      id: `calc:${calc.expression}`,
      title: calc.formatted,
      subtitle: calc.expression,
      category: 'calculator',
      score: 10_000,
      action: { type: 'copy-text', text: calc.clipboard },
    }
  }, [calc, currencyCalc])

  const colorConversionRows = useMemo<SearchResult[]>(() => {
    if (isCompletionInput) return []
    return buildColorConversionResults(value)
  }, [isCompletionInput, value])
  const pendingColorArgumentName = useMemo(() => {
    if (!pendingAction || !isPendingColorConversionAction(pendingAction)) return null
    const textArgument = pendingAction.commandArgumentDefinitions.find(
      (def) => def.type !== 'dropdown'
    )
    return textArgument?.name ?? null
  }, [pendingAction])
  const pendingColorConversionRows = useMemo<SearchResult[]>(() => {
    if (!pendingColorArgumentName) return []
    return buildColorConversionResults(argumentValues[pendingColorArgumentName] ?? '')
  }, [argumentValues, pendingColorArgumentName])
  const pendingInlineActionResult = useMemo<SearchResult | null>(() => {
    if (!pendingAction || !pendingInlineArgument) return null
    return {
      id: `pending:${pendingAction.extensionId}:${pendingAction.commandName}`,
      title: pendingAction.title,
      subtitle: pendingAction.subtitle ?? '',
      category: 'extensions',
      score: 20_000,
      iconDataUrl: pendingAction.iconDataUrl,
      action: {
        type: 'run-extension-command',
        extensionId: pendingAction.extensionId,
        commandName: pendingAction.commandName,
        title: pendingAction.title,
        iconPath: pendingAction.iconPath,
        commandArgumentDefinitions: pendingAction.commandArgumentDefinitions,
        argumentValues,
      },
    }
  }, [argumentValues, pendingAction, pendingInlineArgument])

  const shouldOfferKillPortCommand = useMemo(() => {
    const q = value.trim().toLowerCase()
    if (killPortMode) return true
    if (!/\bkill\b/.test(q) || !/\bport\b/.test(q)) return false
    return true
  }, [killPortMode, value])

  const killPortCommandResult = useMemo<SearchResult | null>(() => {
    if (!shouldOfferKillPortCommand) return null
    const port = killPortValue.trim()
    const indexedResult = searchResults.find(
      (item) => item.id === 'extcmd:raycast.port-manager:kill-listening-process'
    )
    const indexedAction =
      indexedResult?.action.type === 'run-extension-command' ? indexedResult.action : null
    return {
      ...indexedResult,
      id: 'extcmd:raycast.port-manager:kill-listening-process',
      title: 'Kill Process Listening On',
      subtitle: 'Port Manager',
      category: 'extensions',
      score: 20_000,
      action: {
        ...indexedAction,
        type: 'run-extension-command',
        extensionId: 'raycast.port-manager',
        commandName: 'kill-listening-process',
        title: 'Kill Process Listening On',
        argumentValues: port ? { port } : undefined,
        commandArgumentDefinitions: indexedAction?.commandArgumentDefinitions ?? [
          { name: 'port', title: 'Port', placeholder: 'Port', required: true, type: 'text' },
        ],
      },
    }
  }, [killPortValue, searchResults, shouldOfferKillPortCommand])

  const visibleSearchResults = useMemo(() => {
    if (killPortMode) {
      const killProcessResult = searchResults.find(
        (item) => item.id === 'extcmd:raycast.kill-process:index'
      )
      return [
        ...(killPortCommandResult ? [killPortCommandResult] : []),
        ...(killProcessResult ? [killProcessResult] : []),
      ]
    }
    if (pendingAction) {
      if (pendingColorConversionRows.length > 0) return pendingColorConversionRows
      return pendingInlineActionResult ? [pendingInlineActionResult] : []
    }
    const searchRowsWithoutDeepRecommendation = isDeepSearchMode
      ? searchResults
      : searchResults.filter((item) => !item.id.startsWith(DEEP_SEARCH_RESULT_PREFIX))
    const instantDeepSearchRecommendation =
      !isDeepSearchMode && !calcResultRow && colorConversionRows.length === 0
        ? buildDeepSearchRecommendation(
            parsedSearchQuery.query,
            searchRowsWithoutDeepRecommendation
          )
        : null
    const searchRows = instantDeepSearchRecommendation
      ? [instantDeepSearchRecommendation, ...searchRowsWithoutDeepRecommendation]
      : searchRowsWithoutDeepRecommendation
    const pinnedIds = new Set(pinnedCommands.map((pin) => pin.id))
    const homeRankedResults = value.trim()
      ? searchRows
      : [...searchRows].sort((left, right) => {
          const leftScore =
            left.score - (pinnedIds.has(left.id) ? PINNED_HOME_RESULT_SCORE_PENALTY : 0)
          const rightScore =
            right.score - (pinnedIds.has(right.id) ? PINNED_HOME_RESULT_SCORE_PENALTY : 0)
          return rightScore - leftScore
        })
    const withoutDuplicatePort = killPortCommandResult
      ? searchRows.filter(
          (item) => item.id !== 'extcmd:raycast.port-manager:kill-listening-process'
        )
      : homeRankedResults
    const withoutDuplicateColorRows =
      colorConversionRows.length > 0
        ? withoutDuplicatePort.filter((item) => item.category !== 'color-converter')
        : withoutDuplicatePort
    const base = [
      ...(killPortCommandResult ? [killPortCommandResult] : []),
      ...colorConversionRows,
      ...withoutDuplicateColorRows,
    ]
    return calcResultRow ? [calcResultRow, ...base] : base
  }, [
    calcResultRow,
    colorConversionRows,
    killPortCommandResult,
    killPortMode,
    isDeepSearchMode,
    pendingAction,
    pendingColorConversionRows,
    pendingInlineActionResult,
    pinnedCommands,
    parsedSearchQuery.query,
    searchResults,
    value,
  ])
  const visibleSearchCount = visibleSearchResults.length
  const topResult = visibleSearchResults[0] ?? null
  const activeSearchResult = visibleSearchResults[selectedSearch] ?? topResult
  const selectedQuickLookPaths = useMemo(
    () => quickLookPathsForSearchResults(visibleSearchResults, Math.max(selectedSearch, 0)),
    [selectedSearch, visibleSearchResults]
  )
  const selectedQuickLookPath = selectedQuickLookPaths[0] ?? null
  const isKillPortCommandActive =
    activeSearchResult?.id === 'extcmd:raycast.port-manager:kill-listening-process'
  const densePinRail = pinnedCommands.length >= 10
  const canEnterKillPortMode =
    !killPortMode &&
    !pendingAction &&
    !isCompletionInput &&
    !isAiMode &&
    !terminalMode &&
    !killPortArgumentDismissed &&
    isKillPortCommandActive
  const inlineExtensionResult =
    activeSearchResult?.action.type === 'run-extension-command' &&
    activeSearchResult.action.commandArgumentDefinitions?.length === 1 &&
    activeSearchResult.action.commandArgumentDefinitions[0]?.type !== 'dropdown'
      ? activeSearchResult
      : null
  const inlineExtensionArgument =
    inlineExtensionResult?.action.type === 'run-extension-command'
      ? inlineExtensionResult.action.commandArgumentDefinitions?.[0]
      : null
  const canEnterInlineArgumentMode = Boolean(
    inlineExtensionResult &&
      inlineExtensionArgument &&
      !isKillPortCommandActive &&
      !killPortMode &&
      !pendingAction &&
      !isCompletionInput &&
      !isAiMode &&
      !terminalMode
  )
  const pinnedMetaById = useMemo(() => {
    const out = new Map<string, { slot: number }>()
    pinnedCommands.forEach((pin) => {
      out.set(pin.id, { slot: pin.slot })
    })
    return out
  }, [pinnedCommands])

  const suggestions = useMemo(
    () => (isCompletionInput ? pathCompletions : []),
    [isCompletionInput, pathCompletions]
  )

  useEffect(() => {
    let cancelled = false
    void window.tezbar
      .listSearchCandidates()
      .then((items) => {
        if (cancelled) return
        searchCandidatesRef.current = mergeSearchCandidates(searchCandidatesRef.current, items)
        writeCachedSearchCandidates(searchCandidatesRef.current)
      })
      .catch(() => {
        // Persisted candidates and the normal query endpoint remain available.
      })
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    if (!isCompletionInput) {
      setPathCompletions([])
      return
    }

    let cancelled = false
    // Completion reads only the current directory. Send it immediately so the
    // mode switch never waits behind an artificial typing debounce; cleanup
    // below prevents a stale response from replacing a newer query.
    void window.tezbar.completePath(value).then((items) => {
      if (!cancelled) setPathCompletions(items)
    })

    return () => {
      cancelled = true
    }
  }, [isCompletionInput, value])

  useEffect(() => {
    let cancelled = false
    const requestId = ++lastSearchRequestId.current
    // The local candidate cache already renders useful one-character matches.
    // Avoid launching a broad SQLite prefix query for a value that is almost
    // always replaced by the next keystroke a few milliseconds later.
    if (!isDeepSearchMode && normalizedSearchValue.length === 1) {
      setError(null)
      return
    }
    const t = setTimeout(
      () => {
        if (isAiMode || isCompletionInput || terminalMode) {
          return
        }
        if (isDeepSearchMode && deepSearchQuery.length >= 3) {
          setDeepSearchLoadingQuery(deepSearchQuery)
          setError(null)
        }
        void window.tezbar
          .searchAll(normalizedSearchValue)
          .then((items) => {
            if (!cancelled && requestId === lastSearchRequestId.current) {
              const isHome = !normalizedSearchValue
              if (isHome && items.length > 0) {
                homeSearchResultsRef.current = items
                searchCandidatesRef.current = mergeSearchCandidates(
                  searchCandidatesRef.current,
                  items
                )
                setSearchResults(items)
                writeCachedHomeSearchResults(items)
              } else {
                if (!isHome) {
                  queryResultsCacheRef.current.set(normalizedSearchValue.toLowerCase(), {
                    items,
                    cachedAt: Date.now(),
                  })
                  if (!isDeepSearchMode) {
                    searchCandidatesRef.current = mergeSearchCandidates(
                      searchCandidatesRef.current,
                      items.filter((item) => item.category !== 'knowledge')
                    )
                  }
                }
                setSearchResults((current) => (isHome ? current : items))
              }
              setError(null)
              setDeepSearchLoadingQuery(null)
            }
          })
          .catch((searchError: unknown) => {
            if (!cancelled && requestId === lastSearchRequestId.current) {
              const detail =
                searchError instanceof Error ? searchError.message : String(searchError)
              setError(`Search unavailable: ${detail}`)
              setDeepSearchLoadingQuery(null)
            }
          })
      },
      isDeepSearchMode && deepSearchQuery.length >= 3 ? 180 : 80
    )
    return () => {
      cancelled = true
      clearTimeout(t)
    }
  }, [
    deepSearchQuery,
    isAiMode,
    isCompletionInput,
    isDeepSearchMode,
    normalizedSearchValue,
    terminalMode,
  ])

  useEffect(() => {
    if (
      terminalMode ||
      isCompletionInput ||
      searchResults.length === 0 ||
      recentExtensionCommands.length === 0
    )
      return
    if (value.trim()) return
    // When the calc row is present it owns index 0 and should stay
    // selected — typing `2+2` should not jump to a recent app.
    if (calcResultRow) return
    const mostRecent = recentExtensionCommands[0]
    const idx = visibleSearchResults.findIndex((item) => item.id === mostRecent)
    if (idx >= 0 && idx < visibleSearchCount) {
      setSelectedSearch(idx)
    }
  }, [
    isCompletionInput,
    terminalMode,
    recentExtensionCommands,
    searchResults,
    value,
    visibleSearchResults,
    visibleSearchCount,
    calcResultRow,
  ])

  // Keep the selection inside the rendered range whenever the result set
  // changes (e.g. user starts typing a narrower query).
  useEffect(() => {
    if (visibleSearchCount === 0) return
    setSelectedSearch((i) => (i >= visibleSearchCount ? visibleSearchCount - 1 : i))
  }, [visibleSearchCount])

  useEffect(() => {
    if (!isAiMode || filteredChatHistory.length === 0) return
    setSelectedSearch((i) => Math.min(Math.max(-1, i), filteredChatHistory.length - 1))
  }, [filteredChatHistory.length, isAiMode])

  // Slash suggestion list can shrink while the highlight index stays high;
  // keep it in range so Enter always targets a real row.
  useEffect(() => {
    if (suggestions.length === 0) return
    if (isApplicationInput) {
      // Launchpad grid: always keep a tile highlighted so Enter launches it.
      setSelectedSuggestion((i) => (i < 0 ? 0 : Math.min(i, suggestions.length - 1)))
      return
    }
    const firstAppIndex = suggestions.findIndex(
      (item) =>
        item.kind === 'application' &&
        item.applicationAction === 'open-with' &&
        item.section !== 'default'
    )
    if (firstAppIndex >= 0) {
      setSelectedSuggestion(firstAppIndex)
      return
    }
    setSelectedSuggestion((i) => Math.min(Math.max(-1, i), suggestions.length - 1))
  }, [suggestions, isApplicationInput])

  const trackExtensionCommand = (extensionId: string, commandName: string): void => {
    const id = buildRecentExtensionCommandId(extensionId, commandName)
    const next = [id, ...recentExtensionCommands.filter((v) => v !== id)].slice(
      0,
      RECENT_EXTENSION_COMMANDS_LIMIT
    )
    setRecentExtensionCommands(next)
    writeRecentExtensionCommands(next)
  }

  const persistPinnedCommands = (next: PinnedCommand[]): void => {
    const normalized = normalizePinnedSlots(next.map((p) => ({ ...p })))
    setPinnedCommands(normalized)
    writePinnedCommands(normalized)
  }

  const unpinCommandById = (id: string): void => {
    const target = pinnedCommands.find((pin) => pin.id === id)
    if (!target) {
      showActionMsg('That command is not pinned')
      return
    }
    const next = pinnedCommands.filter((pin) => pin.id !== id)
    persistPinnedCommands(next)
    showActionMsg(`Unpinned: ${target.title}`)
  }

  const pinCommand = (result: SearchResult, insertIndex = 0): void => {
    if (!isPinnableSearchResult(result)) {
      showActionMsg('Temporary results can’t be pinned')
      return
    }

    const alreadyPinned = pinnedCommands.some((pin) => pin.id === result.id)
    if (alreadyPinned) {
      showActionMsg('Already pinned. Drag its rail icon to reorder.')
      return
    }

    if (pinnedCommands.length >= MAX_PINNED_COMMANDS) {
      showActionMsg(`Pin limit reached (${MAX_PINNED_COMMANDS})`)
      return
    }

    const to = Math.max(0, Math.min(insertIndex, pinnedCommands.length))
    const next: PinnedCommand[] = [
      ...pinnedCommands.slice(0, to),
      {
        id: result.id,
        title: result.title,
        subtitle: result.subtitle,
        category: result.category,
        action: result.action,
        ...(result.iconDataUrl ? { iconDataUrl: result.iconDataUrl } : {}),
        slot: 1,
      },
      ...pinnedCommands.slice(to),
    ].slice(0, MAX_PINNED_COMMANDS)

    persistPinnedCommands(next)
    showActionMsg(`Pinned: ${result.title}`)
    focusCommandInput()
  }

  const openPinPicker = pinCommand

  const runPinnedCommand = async (pin: PinnedCommand, listIndex: number): Promise<void> => {
    const pinnedResult: SearchResult = {
      id: pin.id,
      title: pin.title,
      subtitle: pin.subtitle,
      category: pin.category,
      score: 1000 - listIndex,
      action: pin.action,
      iconDataUrl: pin.iconDataUrl,
    }
    await runSelectedSearchResult(pinnedResult, listIndex + 1)
  }

  const unpinCommandByIdRef = useRef(unpinCommandById)
  const openPinPickerRef = useRef(openPinPicker)
  const runPinnedCommandRef = useRef(runPinnedCommand)
  unpinCommandByIdRef.current = unpinCommandById
  openPinPickerRef.current = openPinPicker
  runPinnedCommandRef.current = runPinnedCommand

  const updateDraggingPin = (id: string | null): void => {
    draggingPinIdRef.current = id
    setDraggingPinId(id)
  }

  const updatePinDropIndex = (index: number | null): void => {
    pinDropIndexRef.current = index
    setPinDropIndex(index)
  }

  const updatePinUnpinDropActive = (active: boolean): void => {
    pinUnpinDropActiveRef.current = active
    setPinUnpinDropActive(active)
  }

  const updateDraggingSearchResult = (result: SearchResult | null): void => {
    draggingSearchResultRef.current = result
    setDraggingSearchResultId(result?.id ?? null)
  }

  const unpinCommandFromDrag = (id: string): void => {
    const target = pinnedCommands.find((pin) => pin.id === id)
    if (!target) return
    persistPinnedCommands(pinnedCommands.filter((pin) => pin.id !== id))
    showActionMsg(`Unpinned: ${target.title}`)
  }

  const commitPinDrop = (insertIndex = pinDropIndexRef.current): void => {
    const fromId = draggingPinIdRef.current
    const shouldUnpin = pinUnpinDropActiveRef.current
    updateDraggingPin(null)
    updatePinDropIndex(null)
    updatePinUnpinDropActive(false)
    if (!fromId) return
    if (shouldUnpin) {
      unpinCommandFromDrag(fromId)
      return
    }
    if (insertIndex === null) return

    const next = reorderPinnedByInsertIndex(pinnedCommands, fromId, insertIndex)
    if (next === pinnedCommands) return
    persistPinnedCommands(next)
  }

  const pinSearchResultFromDrag = (result: SearchResult, insertIndex: number): void => {
    const existingIndex = pinnedCommands.findIndex((pin) => pin.id === result.id)
    if (existingIndex >= 0) {
      const next = reorderPinnedByInsertIndex(pinnedCommands, result.id, insertIndex)
      if (next !== pinnedCommands) {
        persistPinnedCommands(next)
        showActionMsg(`Moved pin: ${result.title}`)
      }
      return
    }

    pinCommand(result, insertIndex)
  }

  const pinDropIndexFromPointer = (clientY: number): number => {
    const rail = pinRailRef.current
    if (!rail) return pinnedCommands.length
    const items = Array.from(rail.querySelectorAll<HTMLElement>('[data-pin-index]'))
    for (const item of items) {
      const index = Number(item.dataset.pinIndex)
      if (!Number.isFinite(index)) continue
      const rect = item.getBoundingClientRect()
      if (clientY < rect.top + rect.height / 2) return index
    }
    return pinnedCommands.length
  }

  const pinUnpinDropFromPointer = (clientX: number, clientY: number): boolean => {
    if (!isPointerInsidePinRail(clientX, clientY)) return false
    const rail = pinRailRef.current
    if (!rail) return false
    const items = Array.from(rail.querySelectorAll<HTMLElement>('[data-pin-index]'))
    const last = items.at(-1)
    if (!last) return false
    const lastRect = last.getBoundingClientRect()
    return clientY > lastRect.bottom + 8
  }

  const isPointerInsidePinRail = (clientX: number, clientY: number): boolean => {
    const rail = pinRailRef.current
    if (!rail) return false
    const rect = rail.getBoundingClientRect()
    return (
      clientX >= rect.left && clientX <= rect.right && clientY >= rect.top && clientY <= rect.bottom
    )
  }

  const beginSearchResultPointerDrag = (
    event: PointerEvent<HTMLElement>,
    result: SearchResult
  ): void => {
    if (event.button !== 0) return
    if (!isPinnableSearchResult(result)) return

    resultPointerActiveRef.current = true
    resultPointerDraggedRef.current = false
    resultPointerStartRef.current = { x: event.clientX, y: event.clientY }
    draggingSearchResultRef.current = result
    suppressNextSearchResultClickRef.current = false
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  const moveSearchResultPointerDrag = (event: PointerEvent<HTMLElement>): void => {
    if (!resultPointerActiveRef.current || !draggingSearchResultRef.current) return

    const start = resultPointerStartRef.current
    const delta = Math.hypot(event.clientX - start.x, event.clientY - start.y)
    if (!resultPointerDraggedRef.current && delta < SEARCH_RESULT_PIN_DRAG_THRESHOLD) return

    resultPointerDraggedRef.current = true
    suppressNextSearchResultClickRef.current = true
    if (!draggingSearchResultId) {
      setDraggingSearchResultId(draggingSearchResultRef.current.id)
    }

    if (isPointerInsidePinRail(event.clientX, event.clientY)) {
      updatePinDropIndex(pinDropIndexFromPointer(event.clientY))
    } else {
      updatePinDropIndex(null)
    }
  }

  const endSearchResultPointerDrag = (event: PointerEvent<HTMLElement>): void => {
    const result = draggingSearchResultRef.current
    const wasDragged = resultPointerDraggedRef.current

    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }

    resultPointerActiveRef.current = false
    resultPointerDraggedRef.current = false
    updateDraggingSearchResult(null)
    updatePinDropIndex(null)

    if (!wasDragged || !result) return
    event.preventDefault()
    event.stopPropagation()

    if (isPointerInsidePinRail(event.clientX, event.clientY)) {
      pinSearchResultFromDrag(result, pinDropIndexFromPointer(event.clientY))
    }
  }

  const cancelSearchResultPointerDrag = (event: PointerEvent<HTMLElement>): void => {
    resultPointerActiveRef.current = false
    resultPointerDraggedRef.current = false
    draggingSearchResultRef.current = null
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    updateDraggingSearchResult(null)
    updatePinDropIndex(null)
  }

  const beginPinPointerDrag = (
    event: PointerEvent<HTMLElement>,
    pinId: string,
    index: number
  ): void => {
    setPinnedCommandTooltip(null)
    if (event.button !== 0) return
    pinPointerActiveRef.current = true
    pinPointerStartYRef.current = event.clientY
    suppressNextPinClickRef.current = false
    updatePinUnpinDropActive(false)
    updateDraggingPin(pinId)
    updatePinDropIndex(index)
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  const showPinnedCommandTooltip = (element: HTMLElement, pin: PinnedCommand): void => {
    const rect = element.getBoundingClientRect()
    setPinnedCommandTooltip({
      id: pin.id,
      title: pin.title,
      subtitle: pin.subtitle,
      shortcut: pinnedSlotShortcutKey(pin.slot),
      left: rect.right + 10,
      top: rect.top + rect.height / 2,
    })
  }

  const movePinPointerDrag = (event: PointerEvent<HTMLElement>): void => {
    if (!pinPointerActiveRef.current || !draggingPinIdRef.current) return
    if (Math.abs(event.clientY - pinPointerStartYRef.current) > 3) {
      suppressNextPinClickRef.current = true
    }
    const shouldUnpin = pinUnpinDropFromPointer(event.clientX, event.clientY)
    updatePinUnpinDropActive(shouldUnpin)
    if (shouldUnpin) {
      updatePinDropIndex(null)
      return
    }
    if (isPointerInsidePinRail(event.clientX, event.clientY)) {
      updatePinDropIndex(pinDropIndexFromPointer(event.clientY))
    } else {
      updatePinDropIndex(null)
    }
  }

  const endPinPointerDrag = (event: PointerEvent<HTMLElement>): void => {
    if (!pinPointerActiveRef.current) return
    pinPointerActiveRef.current = false
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    commitPinDrop(pinDropIndexRef.current)
  }

  const cancelPinPointerDrag = (event: PointerEvent<HTMLElement>): void => {
    pinPointerActiveRef.current = false
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    updateDraggingPin(null)
    updatePinDropIndex(null)
    updatePinUnpinDropActive(false)
  }

  const isDictating = holdToSpeak.state.kind === 'recording'
  const isTranscribing = holdToSpeak.state.kind === 'transcribing'
  const dictationSupported = holdToSpeak.supported
  const startDictation = holdToSpeak.press
  const stopDictation = holdToSpeak.release

  const speakAnswerText = async (): Promise<void> => {
    if (!streamText.trim()) return
    try {
      const result = await window.tezbar.voiceSpeak(streamText)
      if (!result.ok) {
        showActionMsg('Could not start read-aloud')
      }
    } catch {
      showActionMsg('Could not start read-aloud')
    }
  }

  const clearPendingAction = (): void => {
    setPendingAction(null)
    setArgumentValues({})
    setModelMenuOpen(false)
  }

  const activateDeepSearch = (): void => {
    const query = parseSearchQuery(valueRef.current).query
    setValue(deepSearchInput(query))
    setSearchResultNavigationActive(false)
    setSelectedSearch(-1)
    setFollowSearchSelection(false)
    showActionMsg(query ? 'Deep Search · searching indexed file contents' : null)
    requestAnimationFrame(() => focusCommandInput())
  }

  const enterKillPortMode = (): void => {
    setKillPortArgumentDismissed(false)
    setKillPortQuery(value.trim() || 'kill process')
    killPortValueRef.current = ''
    setKillPortValue('')
    killPortModeRef.current = true
    setKillPortMode(true)
    setSelectedSearch(0)
    setFollowSearchSelection(true)
    showActionMsg('Type a port, then press Enter')
    requestAnimationFrame(() => focusCommandInput())
  }

  const exitKillPortMode = (): void => {
    setKillPortArgumentDismissed(true)
    killPortModeRef.current = false
    setKillPortMode(false)
    killPortValueRef.current = ''
    setKillPortValue('')
    showActionMsg(null)
    requestAnimationFrame(() => focusCommandInput())
  }

  const ensureExtensionInstalled = async (extensionId: string): Promise<void> => {
    const installed = await window.tezbar.extensionList()
    if (installed.some((extension) => extension.id === extensionId)) return
    showActionMsg(`Installing ${extensionId.replace(/^raycast\./, '')}…`)
    await window.tezbar.extensionInstall(extensionId)
  }

  const runKillPortCommand = async (): Promise<void> => {
    const port = killPortValue.trim()
    if (!/^\d{1,5}$/.test(port) || Number(port) <= 0 || Number(port) > 65535) {
      showActionMsg('Enter a valid TCP port')
      return
    }

    await ensureExtensionInstalled('raycast.port-manager')

    const ok = await executeExtensionCommandViaRuntime({
      extensionId: 'raycast.port-manager',
      commandName: 'kill-listening-process',
      argumentValues: { port },
    })
    if (ok) {
      killPortModeRef.current = false
      setKillPortMode(false)
      killPortValueRef.current = ''
      setKillPortValue('')
      setKillPortQuery('')
    }
  }

  const cancelPendingAction = (): void => {
    clearPendingAction()
    focusCommandInput()
  }

  async function executeExtensionCommandViaRuntime(payload: {
    extensionId: string
    commandName: string
    argumentValues?: Record<string, string>
  }): Promise<boolean> {
    try {
      if (payload.extensionId === 'raycast.port-manager') {
        await ensureExtensionInstalled('raycast.port-manager')
      }
      const result = await window.tezbar.extensionRunCommand(payload)
      if (!result.ok) {
        showActionMsg(result.message)
        return false
      }

      if (result.mode === 'view') {
        clearPendingAction()
        setValue('')
        trackExtensionCommand(payload.extensionId, payload.commandName)
        onOpenExtensionRuntime(result)
        return true
      }

      showActionMsg(result.message)
      clearPendingAction()
      setValue('')
      trackExtensionCommand(payload.extensionId, payload.commandName)
      focusCommandInput()
      return true
    } catch (err) {
      showActionMsg(formatExtensionRunError(err))
      return false
    }
  }

  async function submitPendingAction(): Promise<void> {
    if (!pendingAction) return

    const missingRequired = pendingAction.commandArgumentDefinitions.find((def) => {
      if (!def.required) return false
      const current = argumentValues[def.name]
      return !current || current.trim().length === 0
    })

    if (missingRequired) {
      showActionMsg(`Missing required argument: ${missingRequired.title || missingRequired.name}`)
      return
    }

    if (isAiPoweredExtensionAction(pendingAction)) {
      await modelSelectionSaveRef.current
    }

    await executeExtensionCommandViaRuntime({
      extensionId: pendingAction.extensionId,
      commandName: pendingAction.commandName,
      argumentValues,
    })
  }

  async function runSelectedSearchResult(
    result: SearchResult,
    rank = selectedSearch + 1
  ): Promise<void> {
    rememberCurrentLauncherQuery()

    if (
      pendingAction &&
      result.id === `pending:${pendingAction.extensionId}:${pendingAction.commandName}`
    ) {
      requestAnimationFrame(() => focusCommandInput())
      return
    }

    if (
      result.action.type === 'invoke-command' &&
      result.action.commandId === ACTIVATE_DEEP_SEARCH_COMMAND
    ) {
      const requestedQuery = String(result.action.payload?.query ?? parseSearchQuery(value).query)
      setValue(deepSearchInput(requestedQuery))
      setSelectedSearch(0)
      setFollowSearchSelection(true)
      showActionMsg('Deep Search · searching indexed file contents')
      requestAnimationFrame(() => focusCommandInput())
      return
    }

    const recordHandledSearchUsage = async (): Promise<void> => {
      try {
        await window.tezbar.recordSearchActionUsage(result.action, {
          query: value.trim(),
          rank,
          resultId: result.id,
        })
      } catch (error) {
        console.warn('[Search] Failed to record handled usage:', error)
      }
    }

    if (result.action.type === 'invoke-command') {
      clearPendingAction()
      showActionMsg(null)
      setValue('')

      if (result.action.commandId === 'open-providers') {
        await recordHandledSearchUsage()
        onConfigureAi()
        return
      }
      if (result.action.commandId === 'open-settings') {
        await recordHandledSearchUsage()
        onOpenSettings()
        return
      }
      if (result.action.commandId === 'open-extensions-settings') {
        await recordHandledSearchUsage()
        onOpenExtensionsSettings()
        return
      }
      if (result.action.commandId === 'open-extensions') {
        await recordHandledSearchUsage()
        onOpenExtensions()
        return
      }
      if (result.action.commandId === 'open-snippets') {
        await recordHandledSearchUsage()
        onOpenSnippetsPage()
        return
      }
      if (result.action.commandId === 'open-notes') {
        await recordHandledSearchUsage()
        onOpenNotesPage()
        return
      }
      if (result.action.commandId === 'open-emoji-picker') {
        await recordHandledSearchUsage()
        onOpenEmojiPicker()
        return
      }
      if (result.action.commandId === 'open-indexing') {
        await recordHandledSearchUsage()
        onOpenIndexingPage()
        return
      }
      if (result.action.commandId === 'quit-tezbar') {
        await window.tezbar.appQuit()
        return
      }
    }

    const quickNoteIdMatch = /^note:(\d+)$/.exec(result.id)
    if (result.category === 'quick-notes' && quickNoteIdMatch?.[1]) {
      const createdAt = Number(quickNoteIdMatch[1])
      if (Number.isFinite(createdAt)) {
        clearPendingAction()
        showActionMsg(null)
        setValue('')
        await recordHandledSearchUsage()
        onOpenNotesPage({ createdAt })
        return
      }
    }

    if (
      result.action.type === 'run-extension-command' &&
      result.action.extensionId === 'raycast.port-manager' &&
      (result.action.commandName === 'open-ports' ||
        result.action.commandName === 'open-ports-menu-bar')
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
      await recordHandledSearchUsage()
      onOpenPortsPage()
      return
    }

    if (
      result.action.type === 'run-extension-command' &&
      result.action.extensionId === 'raycast.port-manager' &&
      result.action.commandName === 'named-ports'
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
      await recordHandledSearchUsage()
      onOpenPortsPage({ tab: 'named' })
      return
    }

    if (
      result.action.type === 'run-native-command' &&
      result.action.commandId === 'list-listening-ports'
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
      await recordHandledSearchUsage()
      onOpenPortsPage()
      return
    }

    if (
      result.action.type === 'run-native-command' &&
      result.action.commandId === 'show-system-monitor'
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
      await recordHandledSearchUsage()
      onOpenSystemMonitor()
      return
    }

    // The clipboard-history command is a pure UI navigation — we hijack
    // it before it round-trips to main so the launcher flips to the
    // dedicated surface instead of trying to execute a native command.
    if (
      result.action.type === 'run-native-command' &&
      result.action.commandId === 'open-clipboard-history'
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
      await recordHandledSearchUsage()
      onOpenClipboardPage()
      return
    }

    if (
      result.action.type === 'run-native-command' &&
      result.action.commandId === 'open-snippets'
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
      await recordHandledSearchUsage()
      onOpenSnippetsPage()
      return
    }

    if (
      result.action.type === 'run-native-command' &&
      result.action.commandId === 'open-quick-notes'
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
      await recordHandledSearchUsage()
      onOpenNotesPage()
      return
    }

    if (
      result.action.type === 'run-native-command' &&
      result.action.commandId === 'open-emoji-picker'
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
      await recordHandledSearchUsage()
      onOpenEmojiPicker()
      return
    }

    if (result.action.type === 'run-native-command' && result.action.commandId === 'quit-tezbar') {
      clearPendingAction()
      showActionMsg(null)
      await window.tezbar.appQuit()
      return
    }

    if (result.action.type === 'run-extension-command') {
      if (
        result.action.extensionId === 'raycast.port-manager' &&
        result.action.commandName === 'kill-listening-process'
      ) {
        const port = result.action.argumentValues?.port || ''
        if (port) {
          await executeExtensionCommandViaRuntime({
            extensionId: result.action.extensionId,
            commandName: result.action.commandName,
            argumentValues: { port },
          })
          return
        }
        clearPendingAction()
        setKillPortQuery(value.trim() || 'kill port')
        killPortValueRef.current = ''
        setKillPortValue('')
        killPortModeRef.current = true
        setKillPortMode(true)
        showActionMsg('Type a port, then press Enter')
        requestAnimationFrame(() => focusCommandInput())
        return
      }

      const defs =
        Array.isArray(result.action.commandArgumentDefinitions) &&
        result.action.commandArgumentDefinitions.length > 0
          ? result.action.commandArgumentDefinitions
          : result.action.argumentName
            ? [
                {
                  name: 'argument',
                  title: result.action.argumentName,
                  placeholder: result.action.argumentName,
                  required: true,
                  type: 'text',
                } satisfies PendingExtensionArgument,
              ]
            : []

      const requiredDefs = defs.filter((def) => def.required)

      if (requiredDefs.length > 0) {
        const initialValues = defs.reduce(
          (acc, def) => {
            acc[def.name] = ''
            return acc
          },
          {} as Record<string, string>
        )

        setPendingAction({
          extensionId: result.action.extensionId,
          commandName: result.action.commandName,
          title: result.action.title,
          subtitle: result.subtitle,
          iconDataUrl: result.iconDataUrl,
          iconPath: result.action.iconPath,
          commandArgumentDefinitions: defs,
        })
        setArgumentValues(initialValues)
        setSelectedSearch(0)
        setFollowSearchSelection(true)
        const usesInlineArgument =
          defs.length === 1 && defs[0]?.type !== 'dropdown'
        showActionMsg(usesInlineArgument ? null : 'Fill arguments · Enter to run · Esc to cancel')
        if (usesInlineArgument) requestAnimationFrame(() => focusCommandInput())
        return
      }

      await executeExtensionCommandViaRuntime({
        extensionId: result.action.extensionId,
        commandName: result.action.commandName,
        argumentValues: result.action.argumentValues,
      })
      return
    }

    try {
      const r = await window.tezbar.executeSearchAction(result.action, {
        query: value.trim(),
        rank,
        resultId: result.id,
      })
      // A native command that returns content (info / copied / password /
      // toggle) renders as a styled card carrying the command's own icon;
      // plain status blurbs fall through to the default text line.
      const card =
        r.ok && typeof r.kind === 'string' && r.kind
          ? {
              kind: r.kind,
              iconKind:
                result.action.type === 'run-native-command'
                  ? NATIVE_COMMAND_ICON_BY_ID[
                      (result.action as { commandId?: NativeCommandId }).commandId as NativeCommandId
                    ]
                  : undefined,
            }
          : null
      showActionMsg(r.message, card)
      if (r.ok) setValue('')
      if (r.ok) clearPendingAction()
      if (r.ok && result.category === 'snippets' && result.action.type === 'copy-text') {
        void window.tezbar.hide()
      }
    } catch (err) {
      showActionMsg(err instanceof Error ? err.message : 'Action failed')
    }
  }

  const focusCommandInput = (): void => {
    document.getElementById('command-input')?.focus()
  }

  const previewSelectedSearchResult = (): void => {
    if (selectedQuickLookPaths.length === 0 || quickLookPendingRef.current) return
    quickLookPendingRef.current = true
    void window.tezbar
      .quickLookFiles(selectedQuickLookPaths)
      .then((result) => {
        if (!result.ok) showActionMsg(result.message)
      })
      .catch((error: unknown) => {
        showActionMsg(error instanceof Error ? error.message : 'Could not preview file')
      })
      .finally(() => {
        quickLookPendingRef.current = false
      })
  }

  const openTerminalSession = (session: TerminalSessionSummary): void => {
    onOpenTerminal(undefined, session.cwd, session.sessionId)
    setValue('')
    setTerminalMode(false)
    setTerminalPrompt('')
    setTerminalSessions([])
    terminalWorkingDirectoryRef.current = undefined
  }

  const selectedTerminalSession = (): TerminalSessionSummary | null => {
    return terminalSessionAtIndex(orderedTerminalSessions, selectedSearch) ?? null
  }

  const toggleTerminalSessionPin = (): void => {
    const session = selectedTerminalSession()
    if (!session) return
    const next = pinnedTerminalSessionIds.includes(session.sessionId)
      ? pinnedTerminalSessionIds.filter((id) => id !== session.sessionId)
      : [session.sessionId, ...pinnedTerminalSessionIds]
    setPinnedTerminalSessionIds(next)
    writePinnedTerminalSessionIds(next)
    showActionMsg(next.includes(session.sessionId) ? 'Session pinned' : 'Session unpinned')
  }

  const stopSelectedTerminalSession = async (): Promise<void> => {
    const session = selectedTerminalSession()
    if (!session || session.status !== 'running') return
    await window.tezbar.terminalKill(session.sessionId)
    const next = await window.tezbar.terminalList()
    setTerminalSessions(next)
    showActionMsg('Terminal session stopped')
  }

  const deleteSelectedTerminalSession = async (): Promise<void> => {
    const session = selectedTerminalSession()
    if (!session) return
    const deleted = await window.tezbar.terminalDelete(session.sessionId)
    if (!deleted) return
    const nextPins = pinnedTerminalSessionIds.filter((id) => id !== session.sessionId)
    setPinnedTerminalSessionIds(nextPins)
    writePinnedTerminalSessionIds(nextPins)
    setTerminalSessions((current) => current.filter((item) => item.sessionId !== session.sessionId))
    setSelectedSearch((current) => Math.max(0, current - 1))
    showActionMsg('Terminal session deleted')
  }

  const openTerminalSettings = (): void => {
    setTerminalSettingsDraft(readTerminalDefaults())
    setTerminalSettingsOpen(true)
  }

  const saveTerminalSettings = (): void => {
    writeTerminalDefaults(terminalSettingsDraft)
    setTerminalSettingsOpen(false)
    showActionMsg('Terminal defaults saved')
  }

  async function openPathCompletion(item: PathCompletionItem): Promise<void> {
    rememberCurrentLauncherQuery()

    if (!item.path) {
      setValue(item.value)
      requestAnimationFrame(() => focusCommandInput())
      return
    }

    const action =
      item.kind === 'application'
        ? item.applicationAction === 'open'
          ? { type: 'open-app' as const, appName: item.appName ?? item.title }
          : { type: 'open-with-app' as const, path: item.path, appName: item.appName }
        : { type: 'open-file' as const, path: item.path }

    const result = await window.tezbar.executeSearchAction(action, {
      query: value.trim(),
      resultId: item.id,
    })
    showActionMsg(result.message)
    if (result.ok) {
      setValue('')
      setPathCompletions([])
    }
  }

  function completePathInput(item: PathCompletionItem): void {
    if (item.kind === 'application' || item.kind === 'file') {
      void openPathCompletion(item)
      return
    }
    if (item.path) void window.tezbar.recordDirectoryVisit(item.path)
    setValue(item.value)
    setSelectedSuggestion(0)
    requestAnimationFrame(() => focusCommandInput())
  }

  pendingOpenRef.current = pendingAction !== null
  modelMenuOpenRef.current = modelMenuOpen

  useEffect(() => {
    setCommandSurfaceEscapeConsumer(() => {
      if (modelMenuOpenRef.current) {
        setModelMenuOpen(false)
        focusCommandInput()
        return true
      }
      if (pendingOpenRef.current) {
        setPendingAction(null)
        setArgumentValues({})
        showActionMsg(null)
        focusCommandInput()
        return true
      }
      if (killPortModeRef.current) {
        if (killPortValueRef.current) {
          killPortValueRef.current = ''
          setKillPortValue('')
        } else {
          setKillPortArgumentDismissed(true)
          killPortModeRef.current = false
          setKillPortMode(false)
        }
        showActionMsg(null)
        focusCommandInput()
        return true
      }
      if (valueRef.current.startsWith(' ')) {
        // Space prefix = AI mode. Escape first clears a typed prompt, then
        // a second Escape removes the prefix and returns to normal command search.
        setValue(valueRef.current.trim() ? ' ' : '')
        focusCommandInput()
        return true
      }
      if (valueRef.current.startsWith('!')) {
        // Deep Search mirrors the other launcher modes: clear its query first,
        // then let a second Escape return to basic search.
        setValue(parseSearchQuery(valueRef.current).query ? '!' : '')
        focusCommandInput()
        return true
      }
      if (valueRef.current.trimStart().startsWith('`')) {
        // The applications prefix is hidden from the input. Match AI mode's
        // two-step Escape behavior: clear the query first, then leave the mode.
        setValue(valueRef.current.trimStart().slice(1).trim() ? '`' : '')
        focusCommandInput()
        return true
      }
      if (terminalSettingsOpenRef.current) {
        terminalSettingsOpenRef.current = false
        setTerminalSettingsOpen(false)
        focusCommandInput()
        return true
      }
      if (terminalModeRef.current) {
        terminalSessionSelectionActiveRef.current = false
        setValue('')
        setTerminalMode(false)
        setTerminalPrompt('')
        terminalWorkingDirectoryRef.current = undefined
        focusCommandInput()
        return true
      }
      return false
    })
    return () => {
      setCommandSurfaceEscapeConsumer(null)
    }
  }, [])

  useEffect(() => {
    if (!pendingAction) return
    requestAnimationFrame(() => {
      if (pendingInlineArgument) focusCommandInput()
      else argInputRefs.current[0]?.focus()
    })
  }, [pendingAction, pendingInlineArgument])

  useEffect(() => {
    const onGlobalKeyDown = (event: KeyboardEvent): void => {
      const hasCommandMod = event.metaKey || event.ctrlKey
      if (terminalSettingsOpenRef.current) return
      if (terminalModeRef.current && hasCommandMod) {
        const key = event.key.toLowerCase()
        if (key === 'k') {
          event.preventDefault()
          openTerminalSettings()
          return
        }
        if (key === 'x') {
          event.preventDefault()
          void stopSelectedTerminalSession()
          return
        }
        if (key === 'p') {
          event.preventDefault()
          toggleTerminalSessionPin()
          return
        }
        if (key === 'd') {
          event.preventDefault()
          void deleteSelectedTerminalSession()
          return
        }
      }
      if (hasCommandMod) {
        if (event.key.toLowerCase() === 'p' && !event.shiftKey) {
          event.preventDefault()
          if (isAiMode) {
            showActionMsg('Pinned commands are hidden in AI mode')
            return
          }
          const selected = visibleSearchResults[selectedSearch] ?? visibleSearchResults[0] ?? null
          if (!selected) {
            showActionMsg('No command selected to pin or unpin')
            return
          }
          if (selected.category === 'calculator' || selected.category === 'color-converter') {
            showActionMsg('Temporary results can’t be pinned')
            return
          }
          const isPinned = pinnedCommands.some((pin) => pin.id === selected.id)
          if (isPinned) {
            unpinCommandByIdRef.current(selected.id)
          } else {
            openPinPickerRef.current(selected)
          }
          return
        }
      }

      if (event.altKey) {
        const slot = parsePinnedSlotFromKeyEvent(event)
        if (slot !== null && !isAiMode) {
          const pinIndex = pinnedCommands.findIndex((p) => p.slot === slot)
          if (pinIndex >= 0) {
            event.preventDefault()
            const pin = pinnedCommands[pinIndex]
            if (pin) void runPinnedCommandRef.current(pin, pinIndex)
          }
        }
      }
    }

    window.addEventListener('keydown', onGlobalKeyDown)
    return () => window.removeEventListener('keydown', onGlobalKeyDown)
  }, [isAiMode, pinnedCommands, selectedSearch, visibleSearchResults])

  function rememberCurrentLauncherQuery(): void {
    const entry = launcherQueryHistoryEntry(valueRef.current, terminalModeRef.current)
    if (!entry) return

    const history = addLauncherQueryHistoryEntry(entry)
    launcherQueryHistoryRef.current = history
    writeLauncherQueryHistory(history)
  }

  async function onSubmit(e: FormEvent): Promise<void> {
    e.preventDefault()
    setError(null)
    setStreamText('')
    setStreamError(null)
    setIsStreaming(false)
    setEmptyAnswer(false)
    gotAnyTokenRef.current = false
    if (killPortMode) {
      await runKillPortCommand()
      return
    }
    if (pendingAction) {
      if (pendingInlineArgument) {
        await submitPendingAction()
        return
      }
      const pendingFieldFocused = argInputRefs.current.some((el) => el === document.activeElement)
      if (pendingFieldFocused) {
        if (pendingColorConversionRows.length > 0) {
          const selected =
            pendingColorConversionRows[selectedSearch] ?? pendingColorConversionRows[0]
          if (selected) {
            await runSelectedSearchResult(selected, selectedSearch + 1)
            return
          }
        }
        await submitPendingAction()
        return
      }
      clearPendingAction()
      showActionMsg(null)
    }

    rememberCurrentLauncherQuery()

    // AI mode: open the dedicated AI Chat surface and submit the prompt
    // there (multi-turn chat, logs, history — see AgentChatView).
    if (isAiMode) {
      if (showChatHistory) {
        const selectedChat = filteredChatHistory[selectedSearch]
        if (selectedChat) {
          onOpenAiChat({ kind: 'resume', sessionId: selectedChat.id })
          setValue('  ')
          return
        }
      }
      const task = agentTask.trim()
      if (!task) return
      await modelSelectionSaveRef.current
      onOpenAiChat({ kind: 'submit', prompt: task })
      setValue('  ')
      return
    }

    if (terminalMode) {
      const selectedSession = terminalSessionSelectionActiveRef.current
        ? terminalSessionAtIndex(orderedTerminalSessions, selectedSearch)
        : undefined
      if (selectedSession) {
        openTerminalSession(selectedSession)
        return
      }
      const initialCommand = value.trim() || undefined
      onOpenTerminal(
        initialCommand,
        terminalWorkingDirectoryRef.current,
        undefined,
        terminalSettingsDraft
      )
      setValue('')
      setTerminalMode(false)
      setTerminalPrompt('')
      setTerminalSessions([])
      terminalWorkingDirectoryRef.current = undefined
      return
    }

    if (!isCompletionInput && visibleSearchResults.length > 0) {
      const selected = visibleSearchResults[selectedSearch]
      if (selected) {
        await runSelectedSearchResult(selected, selectedSearch + 1)
        return
      }
    }

    if (isCompletionInput && suggestions.length > 0) {
      const idx = Math.min(Math.max(0, selectedSuggestion), suggestions.length - 1)
      const item = suggestions[idx]
      if (item) {
        completePathInput(item)
        return
      }
    }

    if (isSlashInput && value.trim()) {
      await window.tezbar.executeSearchAction(
        { type: 'open-file', path: value.trim() },
        { query: value.trim(), resultId: `path-direct:${value.trim()}` }
      )
      setValue('')
      setPathCompletions([])
      return
    }

    if (isApplicationInput) return

    try {
      const intent = await window.tezbar.query(value)
      if (intent.type === 'answer' || intent.type === 'ai') {
        setIsStreaming(true)
      }
      if (intent.type === 'extension' && intent.name === 'providers') {
        setValue('')
        onConfigureAi()
        return
      }
      if (intent.type === 'extension' && intent.name === 'extensions') {
        setValue('')
        onOpenExtensions()
        return
      }
      if (intent.type === 'extension' && intent.name === 'open-ports') {
        setValue('')
        onOpenPortsPage()
        return
      }
    } catch (err) {
      setIsStreaming(false)
      setError(err instanceof Error ? err.message : 'Query failed')
    }
  }

  // Don't stack the LLM answer card while the user is in AI mode (agent
  // chat opens on its own surface).
  const showAnswer =
    !isAiMode && (isStreaming || Boolean(streamText) || Boolean(streamError) || emptyAnswer)
  const showSuggestions = isCompletionInput && suggestions.length > 0
  const showDeepSearchLoading =
    isDeepSearchMode && deepSearchQuery.length >= 3 && deepSearchLoadingQuery === deepSearchQuery
  const showSearchResults =
    !showDeepSearchLoading &&
    !isCompletionInput &&
    !isAiMode &&
    !terminalMode &&
    visibleSearchCount > 0
  const canQuickLookSelectedResult =
    showSearchResults &&
    (!isDeepSearchMode || searchResultNavigationActive) &&
    canQuickLookSearchResult(
      visibleSearchCount,
      searchResultNavigationActive,
      selectedQuickLookPath
    )
  const terminalSelectedIndex =
    terminalSessionSelectionActiveRef.current &&
    selectedSearch >= 0 &&
    selectedSearch < terminalSessionCount
      ? selectedSearch
      : -1

  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLElement>): void => {
    if (isDeepSearchMode && searchResultNavigationActive && e.key === 'Escape') {
      e.preventDefault()
      setSearchResultNavigationActive(false)
      setFollowSearchSelection(false)
      setSelectedSearch(-1)
      requestAnimationFrame(() => focusCommandInput())
      return
    }

    if (isDeepSearchMode && searchResultNavigationActive && e.key === 'Enter') {
      const selected = visibleSearchResults[selectedSearch]
      if (selected) {
        e.preventDefault()
        void runSelectedSearchResult(selected, selectedSearch + 1)
      }
      return
    }

    if (isPlainSpaceKey(e) && canQuickLookSelectedResult) {
      e.preventDefault()
      e.stopPropagation()
      previewSelectedSearchResult()
      return
    }

    if (isAiMode && (e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 's') {
      e.preventDefault()
      onOpenAiChat({ kind: 'screen' })
      return
    }

    if (
      e.key === 'Enter' &&
      (e.metaKey || e.ctrlKey) &&
      !isAiMode &&
      !isDeepSearchMode &&
      !terminalMode &&
      !killPortMode &&
      !pendingAction &&
      !isCompletionInput
    ) {
      e.preventDefault()
      activateDeepSearch()
      return
    }

    if (killPortMode) {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        if (killPortValue) {
          killPortValueRef.current = ''
          setKillPortValue('')
        } else {
          exitKillPortMode()
        }
        return
      }
      if (e.key === 'Tab') {
        e.preventDefault()
        if (e.shiftKey) exitKillPortMode()
        return
      }
      return
    }

    if (terminalMode && e.key === 'Backspace' && !value) {
      e.preventDefault()
      terminalSessionSelectionActiveRef.current = false
      setTerminalMode(false)
      setTerminalPrompt('')
      terminalWorkingDirectoryRef.current = undefined
      return
    }

    if (isDeepSearchMode && e.key === 'Backspace' && !parseSearchQuery(value).query) {
      e.preventDefault()
      setValue('')
      const homeResults = homeSearchResultsRef.current
      if (homeResults.length > 0) setSearchResults(homeResults)
      setSelectedSearch(0)
      return
    }

    if (isApplicationInput && e.key === 'Backspace' && !slashQuery.slice(1)) {
      e.preventDefault()
      setValue('')
      setPathCompletions([])
      setSelectedSuggestion(0)
      return
    }

    if (
      isSlashInput &&
      (e.key === ' ' || e.code === 'Space') &&
      !e.metaKey &&
      !e.ctrlKey &&
      !e.altKey &&
      value === value.trimEnd()
    ) {
      // Treat the first space after a slash path as an explicit mode switch.
      // Relying on the input event to preserve trailing whitespace proved
      // unreliable in the Tauri webview.
      e.preventDefault()
      setValue(`${value} `)
      setSelectedSuggestion(0)
      return
    }

    if (
      e.key === '>' &&
      !terminalMode &&
      (!value || isSlashInput) &&
      !isAiMode &&
      !killPortMode &&
      !pendingAction
    ) {
      e.preventDefault()
      terminalWorkingDirectoryRef.current = isSlashInput ? value.trim() : undefined
      terminalSessionSelectionActiveRef.current = false
      setValue('')
      setTerminalMode(true)
      setSelectedSearch(-1)
      setFollowSearchSelection(true)
      return
    }

    if (
      !terminalMode &&
      !pendingAction &&
      (!isDeepSearchMode || !searchResultNavigationActive) &&
      !e.metaKey &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.shiftKey &&
      shouldRecallLastLauncherQuery({
        isDeepSearchMode,
        key: e.key,
        selectedResultIndex: selectedSearch,
        value,
        visibleResultCount: visibleSearchCount,
      })
    ) {
      const lastQuery = launcherQueryHistoryRef.current[0]
      if (lastQuery) {
        e.preventDefault()
        setValue(lastQuery)
        setSearchResultNavigationActive(false)
        setFollowSuggestionSelection(true)
        setSelectedSuggestion(0)
        const nextDeepSearchMode = parseSearchQuery(lastQuery).mode === 'deep'
        setFollowSearchSelection(!nextDeepSearchMode)
        setSelectedSearch(lastQuery.startsWith(' ') || nextDeepSearchMode ? -1 : 0)
        return
      }
    }

    if (
      isDeepSearchMode &&
      !searchResultNavigationActive &&
      (e.key === 'ArrowUp' || e.key === 'ArrowDown')
    ) {
      e.preventDefault()
      return
    }

    if (isCompletionInput && suggestions.length) {
      if (isApplicationInput) {
        // Grid navigation: left/right by one, up/down by a full row.
        if (e.key === 'ArrowRight') {
          e.preventDefault()
          setFollowSuggestionSelection(true)
          setSelectedSuggestion((i) => Math.min(i + 1, suggestions.length - 1))
        }
        if (e.key === 'ArrowLeft') {
          e.preventDefault()
          setFollowSuggestionSelection(true)
          setSelectedSuggestion((i) => Math.max(i - 1, 0))
        }
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setFollowSuggestionSelection(true)
          setSelectedSuggestion((i) =>
            Math.min(i + APPLICATIONS_GRID_COLUMNS, suggestions.length - 1)
          )
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault()
          setFollowSuggestionSelection(true)
          setSelectedSuggestion((i) => Math.max(i - APPLICATIONS_GRID_COLUMNS, 0))
        }
      } else {
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setFollowSuggestionSelection(true)
          setSelectedSuggestion((i) => Math.min(i + 1, suggestions.length - 1))
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault()
          setFollowSuggestionSelection(true)
          setSelectedSuggestion((i) => Math.max(i - 1, 0))
        }
      }
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        const item = suggestions[Math.min(Math.max(0, selectedSuggestion), suggestions.length - 1)]
        if (item) void openPathCompletion(item)
        return
      }
    } else if (showChatHistory) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setFollowSearchSelection(true)
        setSelectedSearch((i) => Math.min(i + 1, filteredChatHistory.length - 1))
      }
      if (e.key === 'ArrowUp') {
        if (selectedSearch < 0) return
        e.preventDefault()
        setFollowSearchSelection(true)
        setSelectedSearch((i) => Math.max(i - 1, -1))
      }
    } else if (terminalMode && terminalSessionCount > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        terminalSessionSelectionActiveRef.current = true
        setFollowSearchSelection(true)
        setSelectedSearch((i) => moveTerminalSelectionDown(i, terminalSessionCount))
      }
      if (e.key === 'ArrowUp') {
        if (selectedSearch < 0) return
        e.preventDefault()
        terminalSessionSelectionActiveRef.current = true
        setFollowSearchSelection(true)
        setSelectedSearch((i) => Math.max(i - 1, 0))
      }
    } else if (visibleSearchCount && (!isDeepSearchMode || searchResultNavigationActive)) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setSearchResultNavigationActive(true)
        setFollowSearchSelection(true)
        setSelectedSearch((i) => Math.min(i + 1, visibleSearchCount - 1))
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setSearchResultNavigationActive(true)
        setFollowSearchSelection(true)
        setSelectedSearch((i) => Math.max(i - 1, 0))
      }
    }

    if (e.key === 'h' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      onOpenClipboardPage()
    }

    if (e.key === 'Tab') {
      if (pendingAction) {
        e.preventDefault()
        if (pendingInlineArgument) focusCommandInput()
        else argInputRefs.current[0]?.focus()
        return
      }
      if (isCompletionInput) {
        e.preventDefault()
        const item = suggestions[selectedSuggestion]
        if (item) completePathInput(item)
        return
      }
      if (isDeepSearchMode) {
        e.preventDefault()
        const next = toggleDeepSearchResultNavigation(
          searchResultNavigationActive,
          visibleSearchCount
        )
        setSearchResultNavigationActive(next.navigationActive)
        setFollowSearchSelection(next.navigationActive)
        setSelectedSearch(next.selectedIndex)
        requestAnimationFrame(() => {
          if (next.navigationActive) searchResultsNavigationRef.current?.focus()
          else focusCommandInput()
        })
        return
      }
      if (showChatHistory) {
        e.preventDefault()
        setFollowSearchSelection(true)
        setSelectedSearch((i) =>
          e.shiftKey ? Math.max(i - 1, -1) : Math.min(i + 1, filteredChatHistory.length - 1)
        )
        return
      }
      if (canEnterKillPortMode) {
        e.preventDefault()
        enterKillPortMode()
        return
      }
      if (canEnterInlineArgumentMode && inlineExtensionResult) {
        e.preventDefault()
        void runSelectedSearchResult(inlineExtensionResult, selectedSearch + 1)
      }
    }
    // Enter with completion suggestions: let the form `onSubmit` run so
    // one keypress executes the highlighted file or application.
  }

  async function selectTaskModel(
    task: 'chat' | 'action',
    nextProvider: ProviderId,
    nextModel: string
  ): Promise<void> {
    const providerModels = {
      ...cfg.providerModels,
      [nextProvider]: normalizeProviderModelList(
        nextProvider,
        cfg.providerModels?.[nextProvider] ?? defaultModels(nextProvider)
      ),
    }
    const providerSelectedModels = {
      ...cfg.providerSelectedModels,
      [nextProvider]: nextModel,
    }
    const patch: LlmConfigRecord = {
      ...(task === 'chat' ? { provider: nextProvider, model: nextModel } : {}),
      providerModels,
      providerSelectedModels,
      taskProviderOverrides: { ...cfg.taskProviderOverrides, [task]: nextProvider },
      taskModelOverrides: { ...cfg.taskModelOverrides, [task]: nextModel },
    }
    setCfg((current) => ({ ...current, ...patch }))
    setModelMenuOpen(false)
    const save = (async () => {
      await window.tezbar.setLlmConfig(patch)
      const next = await window.tezbar.getLlmConfig()
      setCfg(next as LlmConfigRecord)
    })().catch((error) => {
      console.error('Failed to save selected model:', error)
    })
    modelSelectionSaveRef.current = save
    await save
    if (modelSelectionSaveRef.current === save) {
      modelSelectionSaveRef.current = null
    }
  }

  async function selectAiModel(nextProvider: ProviderId, nextModel: string): Promise<void> {
    await selectTaskModel('chat', nextProvider, nextModel)
  }

  async function selectExtensionAiModel(
    nextProvider: ProviderId,
    nextModel: string
  ): Promise<void> {
    await selectTaskModel('action', nextProvider, nextModel)
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-2">
      {/* Primary glass card: icon + input */}
      <div className="glass-card relative z-30 shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <form className="relative w-full" onSubmit={(ev) => void onSubmit(ev)}>
          <div className="flex h-7 items-center gap-3">
            <span
              className={cx(
                isAiMode
                  ? 'text-violet-300'
                  : terminalMode
                    ? 'text-emerald-300'
                    : isDeepSearchMode
                      ? 'text-cyan-300'
                      : 'text-ink-3'
              )}
            >
              {isAiMode ? (
                <AiIcon />
              ) : terminalMode ? (
                <TerminalIcon />
              ) : isApplicationInput ? (
                <AppStoreIcon />
              ) : (
                <SearchIcon />
              )}
            </span>
            {isAiMode ? (
              <span
                aria-label="AI mode"
                className="inline-flex shrink-0 items-center gap-1 rounded-tezbar-chip border border-violet-400/40 bg-violet-500/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-violet-200"
              >
                <span className="h-1.5 w-1.5 rounded-full bg-violet-300" />
                AI
              </span>
            ) : isDeepSearchMode ? (
              <span
                aria-label="Deep Search mode"
                className="inline-flex shrink-0 items-center gap-1.5 rounded-tezbar-chip border border-cyan-300/30 bg-cyan-300/10 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-cyan-200"
              >
                <span className="h-1.5 w-1.5 rounded-full bg-cyan-300 shadow-[0_0_8px_rgba(103,232,249,0.7)]" />
                Deep
              </span>
            ) : null}
            {killPortMode ? (
              <>
                <span className="max-w-[220px] truncate font-display text-[15px] text-ink-1">
                  {killPortQuery}
                </span>
                <PortArgumentChip />
              </>
            ) : null}
            {pendingInlineArgument ? (
              <>
                <span className="max-w-[220px] truncate font-display text-[15px] text-ink-1">
                  {value.trim() || pendingAction?.title}
                </span>
                <InlineArgumentChip
                  label={pendingInlineArgument.title || pendingInlineArgument.name}
                />
              </>
            ) : null}
            <div className="relative min-w-0 flex-1 flex items-center">
              {terminalMode ? (
                <span className="shrink-0 font-mono text-[13px] text-emerald-300/80 mr-1 select-none pointer-events-none">
                  {terminalPrompt}
                </span>
              ) : null}
              {!killPortMode && !pendingInlineArgument && !isAiMode && !terminalMode && !value ? (
                <RollingText items={COMMAND_HINTS} />
              ) : null}
              {isSlashInput && !isAiMode ? (
                <span className="path-command-hint" aria-hidden>
                  <span className="path-command-hint__value">{value}</span>
                  <span className="rolling-command-hint__shortcut">{COMMAND_HINT.shortcut}</span>
                  <span className="rolling-command-hint__label">{COMMAND_HINT.label}</span>
                </span>
              ) : null}
              <input
                id="command-input"
                type="text"
                value={
                  pendingInlineArgument
                    ? argumentValues[pendingInlineArgument.name] ?? ''
                    : killPortMode
                    ? killPortValue
                    : isApplicationInput
                      ? slashQuery.slice(1)
                      : isDeepSearchMode
                        ? value.slice(1)
                        : value
                }
                onChange={(e) => {
                  if (pendingInlineArgument) {
                    setArgumentValues((current) => ({
                      ...current,
                      [pendingInlineArgument.name]: e.target.value,
                    }))
                    setSelectedSearch(0)
                    return
                  }
                  if (killPortMode) {
                    const nextPort = e.target.value.replace(/[^\d]/g, '').slice(0, 5)
                    killPortValueRef.current = nextPort
                    setKillPortValue(nextPort)
                    return
                  }
                  if (pendingAction) {
                    clearPendingAction()
                    showActionMsg(null)
                  }
                  setSearchResultNavigationActive(false)
                  const currentSearchValue = searchRequestInput(value)
                  // Keep mode sentinels in state for the search backends, but
                  // never render them beside the visible mode badge.
                  let newValue = isApplicationInput
                    ? `\`${e.target.value}`
                    : isDeepSearchMode
                      ? deepSearchDraftInput(e.target.value)
                      : e.target.value
                  let nextTerminalMode = terminalMode
                  if (!terminalMode) {
                    const separator = newValue.indexOf('>')
                    const pathPrefix = separator > 0 ? newValue.slice(0, separator).trim() : ''
                    if (separator === 0 || (separator > 0 && isAbsoluteTerminalPath(pathPrefix))) {
                      terminalWorkingDirectoryRef.current = pathPrefix || undefined
                      terminalSessionSelectionActiveRef.current = false
                      setTerminalMode(true)
                      nextTerminalMode = true
                      newValue = newValue.slice(separator + 1)
                    }
                  }
                  const nextSearchQuery = parseSearchQuery(newValue)
                  const nextSearchValue = searchRequestInput(newValue)
                  const searchMeaningChanged = currentSearchValue !== nextSearchValue
                  if (searchMeaningChanged) {
                    if (
                      !nextTerminalMode &&
                      nextSearchQuery.mode === 'deep' &&
                      nextSearchQuery.query.length >= 3
                    ) {
                      setDeepSearchLoadingQuery(nextSearchQuery.query)
                      setError(null)
                    } else {
                      setDeepSearchLoadingQuery(null)
                    }
                  }
                  setKillPortArgumentDismissed(false)
                  if (searchMeaningChanged) {
                    if (!nextTerminalMode && newValue.length === 0) {
                      const homeResults = homeSearchResultsRef.current
                      if (homeResults.length > 0) setSearchResults(homeResults)
                    } else if (
                      !nextTerminalMode &&
                      nextSearchQuery.mode === 'deep' &&
                      nextSearchQuery.query
                    ) {
                      setSearchResults([])
                    } else if (
                      !nextTerminalMode &&
                      nextSearchValue &&
                      !newValue.trimStart().startsWith('!') &&
                      !newValue.startsWith(' ') &&
                      !newValue.startsWith('/') &&
                      !newValue.startsWith('`') &&
                      !newValue.endsWith('  ')
                    ) {
                      const normalizedQuery = nextSearchValue.toLowerCase()
                      const cachedEntry = queryResultsCacheRef.current.get(normalizedQuery)
                      const cachedResults =
                        cachedEntry &&
                        Date.now() - cachedEntry.cachedAt <= QUERY_RESULTS_CACHE_TTL_MS
                          ? cachedEntry.items
                          : undefined
                      if (cachedEntry && !cachedResults) {
                        queryResultsCacheRef.current.delete(normalizedQuery)
                      }
                      const immediateResults =
                        cachedResults ??
                        optimisticSearchResults(searchCandidatesRef.current, normalizedQuery)
                      setSearchResults(immediateResults)
                    }
                  }
                  setValue(newValue)
                  const nextIsAiMode =
                    !nextTerminalMode &&
                    nextSearchQuery.mode !== 'deep' &&
                    (newValue.startsWith(' ') || newValue.endsWith('  '))
                  if (
                    searchMeaningChanged ||
                    nextTerminalMode !== terminalMode ||
                    nextIsAiMode !== isAiMode
                  ) {
                    setFollowSuggestionSelection(true)
                    setSelectedSuggestion(0)
                    const nextDeepSearchMode = nextSearchQuery.mode === 'deep'
                    setSearchResultNavigationActive(false)
                    setFollowSearchSelection(!nextDeepSearchMode)
                    setSelectedSearch(
                      nextTerminalMode || nextIsAiMode || nextDeepSearchMode ? -1 : 0
                    )
                  }
                }}
                onKeyDown={handleInputKeyDown}
                aria-label="Search Tezbar or use a shortcut"
                placeholder={
                  pendingInlineArgument
                    ? pendingInlineArgument.placeholder ||
                      pendingInlineArgument.title ||
                      pendingInlineArgument.name
                    : killPortMode
                    ? 'Port'
                    : isAiMode
                      ? 'Ask or command the agent…'
                      : terminalMode
                        ? ''
                        : ''
                }
                autoComplete="off"
                spellCheck={false}
                className="w-full min-w-0 border-0 bg-transparent p-0 font-display text-[15px] font-normal text-ink-1 outline-none ring-0 placeholder:text-ink-4 focus:ring-0"
              />
            </div>
            {canEnterKillPortMode ? (
              <button
                type="button"
                aria-label="Enter port number"
                className="group inline-flex h-7 shrink-0 items-center gap-2 rounded-tezbar-chip px-1 text-left transition hover:bg-white/[0.035]"
                onMouseDown={(event) => event.preventDefault()}
                onClick={enterKillPortMode}
              >
                <PortArgumentChip />
                <span className="min-w-[42px] font-display text-[15px] text-ink-4 transition group-hover:text-ink-3">
                  Port
                </span>
              </button>
            ) : null}
            {canEnterInlineArgumentMode && inlineExtensionResult && inlineExtensionArgument ? (
              <button
                type="button"
                aria-label={`Enter ${inlineExtensionArgument.title || inlineExtensionArgument.name}`}
                className="group inline-flex h-7 shrink-0 items-center gap-2 rounded-tezbar-chip px-1 text-left transition hover:bg-white/[0.035]"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => {
                  void runSelectedSearchResult(inlineExtensionResult, selectedSearch + 1)
                }}
              >
                <InlineArgumentChip
                  label={inlineExtensionArgument.title || inlineExtensionArgument.name}
                />
                <span className="min-w-[52px] font-display text-[15px] text-ink-4 transition group-hover:text-ink-3">
                  {inlineExtensionArgument.title || inlineExtensionArgument.name}
                </span>
              </button>
            ) : null}
            {isAiMode || pendingActionUsesAi ? (
              <ModelPicker
                config={pendingActionUsesAi ? actionModelConfig : cfg}
                open={modelMenuOpen}
                onOpenChange={setModelMenuOpen}
                onSelect={pendingActionUsesAi ? selectExtensionAiModel : selectAiModel}
                onConfigure={onConfigureAi}
                triggerClassName="font-mono leading-none tabular-nums tracking-normal"
              />
            ) : null}
            {dictationSupported && !terminalMode && !pendingInlineArgument ? (
              <button
                type="button"
                className={cx(
                  'inline-flex h-6 min-w-[116px] shrink-0 items-center justify-center rounded-tezbar-chip border px-2 text-[10px] font-medium uppercase leading-none tracking-[0.12em] transition',
                  isDictating
                    ? 'border-rose-400/40 bg-rose-500/20 text-rose-200'
                    : isTranscribing
                      ? 'border-amber-400/40 bg-amber-500/15 text-amber-200'
                      : 'border-white/10 bg-white/[0.03] text-ink-3 hover:text-ink-2'
                )}
                disabled={isTranscribing}
                onMouseDown={(event) => {
                  event.preventDefault()
                  startDictation()
                }}
                onMouseUp={stopDictation}
                onMouseLeave={stopDictation}
                onTouchStart={(event) => {
                  event.preventDefault()
                  startDictation()
                }}
                onTouchEnd={stopDictation}
                title="Hold to speak"
              >
                {isDictating ? (
                  'Listening'
                ) : isTranscribing ? (
                  'Transcribing…'
                ) : (
                  'Hold to speak'
                )}
              </button>
            ) : null}
          </div>
        </form>
      </div>

      {/* Middle column: the pin rail sits on the left; inner panels scroll
          (GlideList, answer, …) instead of this outer region. */}
      <div className="flex min-h-0 flex-1 gap-[var(--s-2)] overflow-hidden pr-0.5">
        {(pinnedCommands.length > 0 || draggingSearchResultId) &&
        !isCompletionInput &&
        !isAiMode &&
        !terminalMode ? (
          <div
            ref={pinRailRef}
            className={cx(
              'tezbar-pin-rail glass-card flex shrink-0 flex-col items-center overflow-hidden',
              densePinRail ? 'w-[50px] p-1.5' : 'w-[50px] px-1.5 py-2',
              draggingPinId ? 'pb-1' : '',
              pinUnpinDropActive ? 'border-rose-300/35 bg-rose-400/[0.04]' : '',
              draggingSearchResultId ? 'border-accent/40 bg-accent/[0.035]' : ''
            )}
          >
            <div
              className={cx(
                'flex min-h-0 flex-col items-center',
                densePinRail ? 'gap-[5px]' : 'gap-2'
              )}
            >
              {pinnedCommands.length === 0 && draggingSearchResultId ? (
                <div
                  aria-hidden
                  className="grid h-9 w-9 place-items-center rounded-tezbar-row border border-dashed border-accent/60 bg-accent/[0.08] font-mono text-[15px] text-accent"
                >
                  +
                </div>
              ) : null}
              {pinnedCommands.map((pin, index) => {
                const pinnedResult: SearchResult = {
                  id: pin.id,
                  title: pin.title,
                  subtitle: pin.subtitle,
                  category: pin.category,
                  score: 1000 - index,
                  action: pin.action,
                  iconDataUrl: pin.iconDataUrl,
                }
                const iconAsset = searchResultIconAsset(pinnedResult)
                const commandIcon = commandIconForResult(pinnedResult)
                return (
                  <div
                    key={`pin:${pin.id}`}
                    data-pin-index={index}
                    onPointerEnter={(event) => showPinnedCommandTooltip(event.currentTarget, pin)}
                    onPointerLeave={() => setPinnedCommandTooltip(null)}
                    onPointerDown={(event) => {
                      beginPinPointerDrag(event, pin.id, index)
                    }}
                    onPointerMove={movePinPointerDrag}
                    onPointerUp={endPinPointerDrag}
                    onPointerCancel={cancelPinPointerDrag}
                    onLostPointerCapture={() => {
                      if (!pinPointerActiveRef.current) return
                      commitPinDrop(pinDropIndexRef.current)
                      pinPointerActiveRef.current = false
                    }}
                    onContextMenu={(event) => {
                      event.preventDefault()
                      unpinCommandById(pin.id)
                    }}
                    className={cx(
                      'group relative flex cursor-grab items-center justify-center rounded-tezbar-row transition active:cursor-grabbing',
                      draggingPinId === pin.id ? 'opacity-45' : 'hover:bg-white/[0.06]'
                    )}
                  >
                    {pinDropIndex === index ? (
                      <span
                        aria-hidden
                        className="pointer-events-none absolute -top-[5px] left-1 right-1 h-0.5 rounded-full bg-accent shadow-[0_0_10px_rgba(139,141,247,0.65)]"
                      />
                    ) : null}
                    {index === pinnedCommands.length - 1 &&
                    pinDropIndex === pinnedCommands.length ? (
                      <span
                        aria-hidden
                        className="pointer-events-none absolute -bottom-[5px] left-1 right-1 h-0.5 rounded-full bg-accent shadow-[0_0_10px_rgba(139,141,247,0.65)]"
                      />
                    ) : null}
                    <button
                      type="button"
                      draggable={false}
                      aria-label={`Run pinned command: ${pin.title} (Option + ${pinnedSlotShortcutKey(pin.slot)})`}
                      aria-describedby={
                        pinnedCommandTooltip?.id === pin.id ? 'pinned-command-tooltip' : undefined
                      }
                      className="grid h-9 w-9 shrink-0 place-items-center rounded-tezbar-row border border-white/10 bg-white/[0.035] transition hover:border-white/20 hover:bg-white/[0.08]"
                      onClick={(event) => {
                        if (suppressNextPinClickRef.current) {
                          event.preventDefault()
                          event.stopPropagation()
                          suppressNextPinClickRef.current = false
                          return
                        }
                        void runPinnedCommand(pin, index)
                      }}
                      onFocus={(event) => showPinnedCommandTooltip(event.currentTarget, pin)}
                      onBlur={() => setPinnedCommandTooltip(null)}
                    >
                      <ListItemIcon
                        kind={pin.category}
                        iconDataUrl={pin.iconDataUrl}
                        assetKind={iconAsset?.kind}
                        assetPath={iconAsset?.path}
                        commandIcon={commandIcon}
                      />
                    </button>
                    <span
                      className={cx(
                        'absolute bottom-0 right-0 grid place-items-center rounded-[5px] border border-white/10 bg-[#14161c] px-1 font-mono leading-none text-ink-3 transition hover:text-ink-1',
                        densePinRail
                          ? 'h-[15px] min-w-[17px] text-[9px]'
                          : 'h-[15px] min-w-[15px] text-[9px]'
                      )}
                    >
                      {pinnedSlotShortcutKey(pin.slot)}
                    </span>
                  </div>
                )
              })}
            </div>
            {draggingPinId ? (
              <div
                aria-hidden
                className={cx(
                  'pointer-events-none absolute bottom-1 left-1 right-1 grid h-8 place-items-center rounded-tezbar-row border border-dashed transition',
                  pinUnpinDropActive
                    ? 'border-rose-300/55 bg-rose-400/15 text-rose-200 shadow-[0_0_14px_rgba(251,113,133,0.18)]'
                    : 'border-white/10 bg-white/[0.025] text-ink-4/70'
                )}
              >
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                  <path
                    d="M3.25 4.25h7.5M5.25 4.25V3h3.5v1.25M4.25 4.25l.5 7h4.5l.5-7M6 6.25v3M8 6.25v3"
                    stroke="currentColor"
                    strokeWidth="1.2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              </div>
            ) : null}
          </div>
        ) : null}

        <div className="flex min-h-0 flex-1 flex-col gap-[var(--s-2)] overflow-hidden">
          {/* File and application completion suggestions */}
          {showSuggestions ? (
            <div
              className="glass-card animate-tezbar-scale-in flex min-h-0 flex-1 flex-col overflow-hidden px-2 py-2"
              onWheelCapture={() => setFollowSuggestionSelection(false)}
              onMouseLeave={() => {
                setFollowSuggestionSelection(false)
                setSelectedSuggestion(-1)
              }}
            >
              {isApplicationInput ? (
                <ApplicationGrid
                  items={suggestions}
                  selectedIndex={selectedSuggestion}
                  columns={APPLICATIONS_GRID_COLUMNS}
                  onHover={(i) => {
                    setFollowSuggestionSelection(false)
                    setSelectedSuggestion(i)
                  }}
                  onActivate={(item) => completePathInput(item)}
                />
              ) : (
                <GlideList
                  selectedIndex={selectedSuggestion}
                  itemCount={suggestions.length}
                  followSelected={followSuggestionSelection}
                  className="min-h-0 flex-1 overflow-y-auto"
                >
                  {suggestions.map((item, i) => {
                    const iconAsset = completionIconAsset(item)
                    const sectionLabel =
                      i === 0 || suggestions[i - 1]?.section !== item.section
                        ? pathCompletionSectionLabel(item.section)
                        : null
                    return (
                      <li key={item.id} className="relative z-[1]">
                        {sectionLabel ? (
                          <div className="px-3 pb-1 pt-2 text-[9.5px] font-bold uppercase tracking-[0.16em] text-ink-4">
                            {sectionLabel}
                          </div>
                        ) : null}
                        <button
                          type="button"
                          className="relative flex w-full items-center justify-between gap-3 rounded-tezbar-row px-3 py-2 text-left text-[13px] text-ink-2 transition hover:text-ink-1"
                          onMouseMove={() => {
                            setFollowSuggestionSelection(false)
                            setSelectedSuggestion(i)
                          }}
                          onMouseDown={(ev) => ev.preventDefault()}
                          onClick={() => completePathInput(item)}
                        >
                          <span className="flex min-w-0 flex-1 items-center gap-3">
                            <ListItemIcon
                              kind={item.kind}
                              iconDataUrl={item.iconDataUrl}
                              assetKind={iconAsset?.kind}
                              assetPath={iconAsset?.path}
                            />
                            <span className="min-w-0 flex-1">
                              <span className="block truncate font-mono text-[12.5px] tracking-tight text-ink-1">
                                {item.title}
                              </span>
                              <span className="mt-0.5 block truncate text-[10.5px] text-ink-4">
                                {item.subtitle}
                              </span>
                            </span>
                          </span>
                          <span className="ml-3 shrink-0 text-[9.5px] font-medium uppercase tracking-[0.14em] text-ink-4">
                            {item.badge ??
                              (item.kind === 'directory'
                                ? 'Folder'
                                : item.kind === 'application'
                                  ? 'Open With'
                                  : 'File')}
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </GlideList>
              )}
            </div>
          ) : null}

          {showDeepSearchLoading ? (
            <div
              className="glass-card animate-tezbar-scale-in flex min-h-0 flex-1 items-center justify-center overflow-hidden px-6 py-8"
              role="status"
              aria-live="polite"
              aria-label={`Deep Search is searching for ${deepSearchQuery}`}
            >
              <div className="flex w-full max-w-[520px] flex-col items-center">
                <div className="relative mb-5 grid h-14 w-14 place-items-center">
                  <span className="absolute inset-0 animate-pulse rounded-full border border-cyan-300/15 bg-cyan-300/[0.035] shadow-[0_0_28px_rgba(103,232,249,0.08)]" />
                  <span className="absolute inset-[7px] animate-spin rounded-full border border-transparent border-r-cyan-300/30 border-t-cyan-200/80 [animation-duration:1.15s]" />
                  <svg
                    width="19"
                    height="19"
                    viewBox="0 0 20 20"
                    fill="none"
                    aria-hidden
                    className="relative text-cyan-200"
                  >
                    <circle cx="8.5" cy="8.5" r="5.25" stroke="currentColor" strokeWidth="1.6" />
                    <path
                      d="m12.4 12.4 4.1 4.1"
                      stroke="currentColor"
                      strokeWidth="1.6"
                      strokeLinecap="round"
                    />
                  </svg>
                </div>

                <p className="font-display text-[14px] font-semibold tracking-[0.01em] text-ink-1">
                  Searching indexed files
                </p>
                <p className="mt-1 max-w-[380px] truncate text-center text-[11px] text-ink-4">
                  Matching text, OCR, and vector embeddings for “{deepSearchQuery}”
                </p>

                <div className="mt-7 w-full space-y-2" aria-hidden>
                  {[82, 68, 91].map((titleWidth, index) => (
                    <div
                      key={titleWidth}
                      className="flex items-center gap-3 rounded-tezbar-row border border-white/[0.045] bg-white/[0.018] px-3 py-2.5"
                    >
                      <span className="h-8 w-8 shrink-0 animate-pulse rounded-tezbar-row border border-cyan-200/[0.08] bg-cyan-200/[0.035]" />
                      <span className="min-w-0 flex-1 space-y-2">
                        <span
                          className="block h-2.5 animate-tezbar-shimmer rounded-full bg-[linear-gradient(90deg,rgba(255,255,255,0.035),rgba(103,232,249,0.10),rgba(255,255,255,0.035))] bg-[length:200%_100%]"
                          style={{ width: `${titleWidth}%` }}
                        />
                        <span
                          className="block h-2 animate-tezbar-shimmer rounded-full bg-[linear-gradient(90deg,rgba(255,255,255,0.025),rgba(255,255,255,0.07),rgba(255,255,255,0.025))] bg-[length:200%_100%]"
                          style={{
                            width: `${Math.max(46, titleWidth - 18 - index * 4)}%`,
                            animationDelay: `${index * 110}ms`,
                          }}
                        />
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          ) : null}

          {/* Pending extension action form */}
          {pendingAction && !pendingInlineArgument ? (
            <form
              onSubmit={(ev) => {
                ev.preventDefault()
                void submitPendingAction()
              }}
              className="glass-card relative z-20 animate-tezbar-scale-in p-2"
            >
              <div className="relative rounded-tezbar-row border border-white/[0.17] bg-white/[0.065] px-4 py-3 shadow-[inset_0_1px_0_rgba(255,255,255,0.055),0_14px_30px_rgba(0,0,0,0.16)]">
                <span
                  aria-hidden
                  className="absolute bottom-3 left-0 top-3 w-[3px] rounded-full bg-emerald-300/85 shadow-[0_0_12px_rgba(110,231,183,0.3)]"
                />

                <div className="flex min-w-0 items-center gap-3">
                  <ListItemIcon kind="extensions" iconDataUrl={pendingAction.iconDataUrl} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px] font-medium text-ink-1">
                      {pendingAction.title}
                    </span>
                    <span className="mt-0.5 block truncate text-[11px] text-ink-3">
                      <span className="text-ink-4">extensions</span>
                      {pendingAction.subtitle ? (
                        <>
                          <span className="mx-1.5 text-ink-4">·</span>
                          {pendingAction.subtitle}
                        </>
                      ) : null}
                    </span>
                  </span>
                  <span className="flex shrink-0 items-center gap-2">
                    {pendingActionUsesAi ? (
                      <span className="inline-flex h-6 items-center gap-1.5 rounded-tezbar-chip border border-violet-300/15 bg-violet-300/[0.07] px-2 text-[9px] font-semibold uppercase tracking-[0.11em] text-violet-200/80">
                        <span className="h-1.5 w-1.5 rounded-full bg-violet-300/80" />
                        AI powered
                      </span>
                    ) : null}
                    <Kbd>↵</Kbd>
                  </span>
                </div>

                <div className="mt-3 space-y-2.5 rounded-tezbar-row border border-white/[0.075] bg-black/20 p-3 shadow-[inset_0_1px_8px_rgba(0,0,0,0.18)]">
                  {pendingAction.commandArgumentDefinitions.map((arg, index) => {
                    const fieldType = arg.type === 'dropdown' ? 'dropdown' : 'text'
                    const label = arg.title || arg.name
                    const placeholder = arg.placeholder || arg.title || arg.name
                    const currentValue = argumentValues[arg.name] ?? ''

                    const onKeyDown = (
                      e: React.KeyboardEvent<HTMLInputElement | HTMLSelectElement>
                    ): void => {
                      if (e.key === 'Escape') {
                        e.preventDefault()
                        cancelPendingAction()
                        return
                      }
                      if (e.key === 'Tab') {
                        e.preventDefault()
                        const nextIndex = e.shiftKey ? index - 1 : index + 1
                        if (
                          nextIndex >= 0 &&
                          nextIndex < pendingAction.commandArgumentDefinitions.length
                        ) {
                          argInputRefs.current[nextIndex]?.focus()
                        } else {
                          focusCommandInput()
                        }
                        return
                      }
                      if (e.key === 'Enter') {
                        e.preventDefault()
                        if (pendingColorConversionRows.length > 0) {
                          const selected =
                            pendingColorConversionRows[selectedSearch] ??
                            pendingColorConversionRows[0]
                          if (selected) {
                            void runSelectedSearchResult(selected, selectedSearch + 1)
                            return
                          }
                        }
                        void submitPendingAction()
                      }
                    }

                    return (
                      <label
                        key={`${pendingAction.commandName}:${arg.name}`}
                        className="block min-w-0"
                      >
                        <span className="mb-1.5 flex items-center gap-1 text-[9.5px] font-semibold uppercase tracking-[0.13em] text-ink-3">
                          {label}
                          {arg.required ? (
                            <span className="text-emerald-300" aria-label="required">
                              •
                            </span>
                          ) : null}
                        </span>
                        {fieldType === 'dropdown' ? (
                          <SelectField
                            ref={(el) => {
                              argInputRefs.current[index] = el
                            }}
                            value={currentValue}
                            onChange={(e) => {
                              const next = e.target.value
                              setArgumentValues((prev) => ({ ...prev, [arg.name]: next }))
                              if (arg.name === pendingColorArgumentName) setSelectedSearch(0)
                            }}
                            onKeyDown={onKeyDown}
                            className="h-10 min-w-0 bg-black/25 px-3 text-[13px]"
                          >
                            <option value="">Select…</option>
                            {(arg.data || []).map((option) => {
                              const optionValue = String(option?.value ?? '')
                              const optionTitle = option?.title || optionValue
                              return (
                                <option key={`${arg.name}:${optionValue}`} value={optionValue}>
                                  {optionTitle}
                                </option>
                              )
                            })}
                          </SelectField>
                        ) : (
                          <TextField
                            ref={(el) => {
                              argInputRefs.current[index] = el
                            }}
                            type={fieldType}
                            value={currentValue}
                            onChange={(e) => {
                              const next = e.target.value
                              setArgumentValues((prev) => ({ ...prev, [arg.name]: next }))
                              if (arg.name === pendingColorArgumentName) setSelectedSearch(0)
                            }}
                            onKeyDown={onKeyDown}
                            placeholder={placeholder}
                            autoComplete="off"
                            spellCheck={false}
                            className="h-10 min-w-0 bg-black/25 px-3 text-[13px]"
                          />
                        )}
                      </label>
                    )
                  })}
                </div>

                <div className="mt-2.5 flex items-center justify-between gap-3 px-0.5 text-[10px] text-ink-4">
                  <span>Enter to run this command</span>
                  <span className="flex items-center gap-1.5">
                    <Kbd>Esc</Kbd>
                    <span>cancel</span>
                  </span>
                </div>
              </div>
            </form>
          ) : null}

          {/* AI mode chat history */}
          {showChatHistory ? (
            <div
              className="flex min-h-0 flex-1 flex-col"
              onWheelCapture={() => setFollowSearchSelection(false)}
              onMouseLeave={() => {
                setFollowSearchSelection(false)
              }}
            >
              <div className="glass-card animate-tezbar-scale-in flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden px-2 py-2">
                <div className="mb-2 px-3 pt-1 text-[10px] font-bold uppercase tracking-[0.12em] text-violet-300">
                  Recent Chats
                </div>
                <GlideList
                  selectedIndex={selectedSearch}
                  itemCount={filteredChatHistory.length}
                  followSelected={followSearchSelection}
                >
                  {filteredChatHistory.map((chat, i) => (
                    <li key={chat.id} className="relative z-[1]">
                      <button
                        type="button"
                        className="group relative flex w-full items-center gap-3 rounded-tezbar-row px-3 py-2 text-left transition"
                        onMouseMove={() => {
                          setFollowSearchSelection(false)
                          setSelectedSearch(i)
                        }}
                        onMouseDown={(ev) => ev.preventDefault()}
                        onClick={() => {
                          onOpenAiChat({ kind: 'resume', sessionId: chat.id })
                          setValue('  ')
                        }}
                      >
                        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-violet-500/10 text-violet-300 group-hover:bg-violet-500/20">
                          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                            <path
                              d="M7 11.5c2.485 0 4.5-2.015 4.5-4.5S9.485 2.5 7 2.5 2.5 4.515 2.5 7c0 1.05.36 2.015.964 2.783L3 11l1.217-.464c.768.604 1.733.964 2.783.964z"
                              stroke="currentColor"
                              strokeWidth="1.2"
                              strokeLinecap="round"
                              strokeLinejoin="round"
                            />
                          </svg>
                        </div>
                        <div className="flex min-w-0 flex-1 flex-col">
                          <span className="truncate text-[13px] font-medium text-ink-1">
                            {chat.title || 'Untitled Chat'}
                          </span>
                          <span className="truncate text-[11px] text-ink-3">
                            {chat.preview || 'No preview available'}
                          </span>
                        </div>
                        <div className="shrink-0 text-[10px] font-medium text-ink-4">
                          {new Date(chat.updatedAt).toLocaleDateString([], {
                            month: 'short',
                            day: 'numeric',
                          })}
                        </div>
                      </button>
                    </li>
                  ))}
                </GlideList>
              </div>
            </div>
          ) : null}

          {/* Terminal sessions */}
          {showTerminalSessions ? (
            <div
              className="flex min-h-0 flex-1 flex-col"
              onWheelCapture={() => setFollowSearchSelection(false)}
              onMouseLeave={() => {
                setFollowSearchSelection(false)
                setSelectedSearch(-1)
              }}
            >
              <div className="glass-card animate-tezbar-scale-in flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden px-2 py-2">
                <div className="mb-2 flex items-center justify-between px-3 pt-1">
                  <div>
                    <div className="text-[10px] font-bold uppercase tracking-[0.16em] text-emerald-300">
                      Terminal Sessions
                    </div>
                    <div className="mt-0.5 text-[10.5px] text-ink-4">
                      {terminalSessions.filter((session) => session.status === 'running').length}{' '}
                      running
                    </div>
                  </div>
                  {terminalSelectedIndex < 0 ? (
                    <span className="rounded-tezbar-chip border border-white/10 bg-white/[0.03] px-2 py-1 text-[10px] text-ink-4">
                      ↓ selects a session · Enter starts new
                    </span>
                  ) : null}
                </div>
                <GlideList
                  selectedIndex={terminalSelectedIndex}
                  itemCount={terminalSessionCount === 0 ? 1 : terminalSessionCount}
                  followSelected={followSearchSelection}
                  className="min-h-0 flex-1 overflow-y-auto"
                >
                  {terminalSessionCount === 0 ? (
                    <li className="relative z-[1]">
                      <div className="mx-1 rounded-tezbar-row border border-white/[0.06] bg-white/[0.03] px-3 py-3 text-[12px] text-ink-4">
                        No saved terminal sessions yet. Press Enter to start a new terminal.
                      </div>
                    </li>
                  ) : null}
                  {orderedTerminalSessions.map((session, i) => {
                    const running = session.status === 'running'
                    return (
                      <li key={session.sessionId} className="relative z-[1]">
                        <button
                          type="button"
                          className={cx(
                            'relative flex w-full items-center justify-between gap-3 rounded-tezbar-row px-3 py-2.5 text-left transition',
                            i === terminalSelectedIndex
                              ? 'bg-emerald-400/[0.08] text-ink-1'
                              : 'text-ink-3 hover:bg-white/[0.05] hover:text-ink-1'
                          )}
                          onMouseMove={() => {
                            setFollowSearchSelection(false)
                            setSelectedSearch(i)
                          }}
                          onMouseDown={(ev) => ev.preventDefault()}
                          onClick={() => openTerminalSession(session)}
                        >
                          <span className="flex min-w-0 flex-1 items-center gap-3">
                            <span
                              className={cx(
                                'grid h-8 w-8 shrink-0 place-items-center rounded-tezbar-row border',
                                running
                                  ? 'border-emerald-300/25 bg-emerald-300/10 text-emerald-200'
                                  : 'border-white/10 bg-white/[0.035] text-ink-4'
                              )}
                            >
                              <TerminalIcon />
                            </span>
                            <span className="min-w-0 flex-1">
                              <span className="flex min-w-0 items-center gap-2">
                                <span
                                  className={cx(
                                    'h-1.5 w-1.5 shrink-0 rounded-full',
                                    running
                                      ? 'bg-emerald-300 shadow-[0_0_12px_rgba(110,231,183,0.75)]'
                                      : 'bg-white/25'
                                  )}
                                />
                                <span className="truncate text-[13px] font-semibold text-ink-1">
                                  {session.name}
                                </span>
                                {pinnedTerminalSessionIds.includes(session.sessionId) ? (
                                  <span
                                    className="shrink-0 text-[10px] text-amber-200"
                                    aria-label="Pinned"
                                  >
                                    ★
                                  </span>
                                ) : null}
                              </span>
                              <span className="mt-0.5 block truncate font-mono text-[10.5px] text-ink-4">
                                {terminalSessionSubtitle(session)}
                              </span>
                            </span>
                          </span>
                          <span className="shrink-0 text-right">
                            <span
                              className={cx(
                                'block text-[10px] font-semibold uppercase tracking-[0.12em]',
                                running ? 'text-emerald-300' : 'text-ink-4'
                              )}
                            >
                              {running ? 'Running' : 'Saved'}
                            </span>
                            <span className="mt-1 block font-mono text-[10px] text-ink-4">
                              {terminalSessionAge(session.updatedAt)}
                            </span>
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </GlideList>
              </div>
            </div>
          ) : null}

          {/* Search results — grows to fill space below pinned / other chrome */}
          {showSearchResults ? (
            <div
              ref={searchResultsNavigationRef}
              tabIndex={isDeepSearchMode ? -1 : undefined}
              role={isDeepSearchMode ? 'region' : undefined}
              aria-label={isDeepSearchMode ? 'Deep Search results' : undefined}
              onKeyDown={isDeepSearchMode ? handleInputKeyDown : undefined}
              className="flex min-h-0 flex-1 flex-col focus:outline-none"
              onWheelCapture={() => setFollowSearchSelection(false)}
              onMouseLeave={() => {
                setFollowSearchSelection(false)
                if (!searchResultNavigationActive) setSelectedSearch(-1)
              }}
            >
              <div className="glass-card animate-tezbar-scale-in flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden px-2 py-2">
                <GlideList
                  selectedIndex={selectedSearch}
                  itemCount={visibleSearchCount}
                  followSelected={followSearchSelection}
                  className="min-h-0 flex-1 overflow-y-auto"
                >
                  {visibleSearchResults.map((item, i) => {
                    const iconAsset = searchResultIconAsset(item)
                    const commandIcon = commandIconForResult(item)
                    const pinnedMeta = pinnedMetaById.get(item.id)
                    const canPinResult = isPinnableSearchResult(item)
                    const isCalc = item.category === 'calculator'
                    const isColorConversion = item.category === 'color-converter'
                    const isCurrencyRow = isCalc && item.id.startsWith('currency:')
                    const colorSwatch =
                      isColorConversion && item.action.type === 'copy-text'
                        ? item.action.text
                        : item.title
                    return (
                      <li key={item.id} className="relative z-[1]">
                        <button
                          type="button"
                          className={cx(
                            'relative flex w-full items-center justify-between gap-3 rounded-tezbar-row text-left transition',
                            isCalc || isColorConversion ? 'px-3 py-2.5' : 'px-3 py-2',
                            canPinResult ? 'cursor-grab active:cursor-grabbing' : ''
                          )}
                          onPointerDown={(event) => {
                            beginSearchResultPointerDrag(event, item)
                          }}
                          onPointerMove={moveSearchResultPointerDrag}
                          onPointerUp={endSearchResultPointerDrag}
                          onPointerCancel={cancelSearchResultPointerDrag}
                          onLostPointerCapture={() => {
                            if (!resultPointerActiveRef.current) return
                            resultPointerActiveRef.current = false
                            resultPointerDraggedRef.current = false
                            updateDraggingSearchResult(null)
                            updatePinDropIndex(null)
                          }}
                          onMouseMove={() => {
                            if (!isDeepSearchMode) setSearchResultNavigationActive(false)
                            setFollowSearchSelection(false)
                            setSelectedSearch(i)
                          }}
                          onMouseDown={(ev) => ev.preventDefault()}
                          onClick={(event) => {
                            if (suppressNextSearchResultClickRef.current) {
                              event.preventDefault()
                              event.stopPropagation()
                              suppressNextSearchResultClickRef.current = false
                              return
                            }
                            setSelectedSearch(i)
                            void runSelectedSearchResult(item, i + 1)
                          }}
                        >
                          {pinnedMeta ? (
                            <span
                              aria-hidden
                              className="absolute bottom-2 left-0 top-2 w-[3px] rounded-full bg-amber-300/80 shadow-[0_0_12px_rgba(252,211,77,0.35)]"
                            />
                          ) : null}
                          {isCalc || isColorConversion ? (
                            <>
                              <span className="flex min-w-0 flex-1 items-center gap-2.5">
                                <span
                                  aria-hidden
                                  className="grid h-7 w-7 shrink-0 place-items-center rounded-tezbar-chip border border-white/10 bg-white/[0.04] text-ink-3"
                                >
                                  {isColorConversion ? (
                                    <span
                                      className="h-[18px] w-[18px] rounded-full border border-white/30 shadow-[0_0_14px_rgba(255,255,255,0.18)]"
                                      style={{ background: colorSwatch }}
                                    />
                                  ) : isCurrencyRow ? (
                                    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                                      <circle
                                        cx="7"
                                        cy="7"
                                        r="5.25"
                                        stroke="currentColor"
                                        strokeWidth="1.1"
                                      />
                                      <path
                                        d="M9 5.25c-.4-.55-1.1-.95-2-.95-1.1 0-2 .55-2 1.4 0 2 4 1 4 3 0 .85-.9 1.4-2 1.4-.9 0-1.6-.4-2-.95"
                                        stroke="currentColor"
                                        strokeWidth="1.1"
                                        strokeLinecap="round"
                                      />
                                      <path
                                        d="M7 3.25v7.5"
                                        stroke="currentColor"
                                        strokeWidth="1.1"
                                        strokeLinecap="round"
                                      />
                                    </svg>
                                  ) : (
                                    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                                      <rect
                                        x="2.5"
                                        y="1.5"
                                        width="9"
                                        height="11"
                                        rx="1.5"
                                        stroke="currentColor"
                                        strokeWidth="1.1"
                                      />
                                      <rect
                                        x="4.25"
                                        y="3.25"
                                        width="5.5"
                                        height="2"
                                        rx="0.4"
                                        fill="currentColor"
                                      />
                                      <circle cx="5" cy="7.5" r="0.6" fill="currentColor" />
                                      <circle cx="7" cy="7.5" r="0.6" fill="currentColor" />
                                      <circle cx="9" cy="7.5" r="0.6" fill="currentColor" />
                                      <circle cx="5" cy="9.75" r="0.6" fill="currentColor" />
                                      <circle cx="7" cy="9.75" r="0.6" fill="currentColor" />
                                      <circle cx="9" cy="9.75" r="0.6" fill="currentColor" />
                                    </svg>
                                  )}
                                </span>
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate font-mono text-[15px] font-semibold tabular-nums text-ink-1">
                                    {item.title}
                                  </span>
                                  <span className="mt-0.5 block truncate text-[11px] text-ink-3">
                                    <span className="text-ink-4">
                                      {isColorConversion
                                        ? 'Color'
                                        : isCurrencyRow
                                          ? 'Currency'
                                          : 'Calculator'}
                                    </span>
                                    <span className="mx-1.5 text-ink-4">·</span>
                                    <span className="font-mono">{item.subtitle}</span>
                                  </span>
                                </span>
                              </span>
                              <span className="shrink-0 flex items-center gap-1.5 text-[10px] font-mono text-ink-3">
                                <Kbd>↵</Kbd>
                                <span>copy</span>
                              </span>
                            </>
                          ) : (
                            <>
                              <span className="flex min-w-0 flex-1 items-center gap-3">
                                <ListItemIcon
                                  kind={item.category}
                                  iconDataUrl={item.iconDataUrl}
                                  assetKind={iconAsset?.kind}
                                  assetPath={iconAsset?.path}
                                  commandIcon={commandIcon}
                                />
                                <span className="min-w-0 flex-1">
                                  <span className="block truncate text-[13px] font-medium text-ink-1">
                                    {item.title}
                                  </span>
                                  <span className="mt-0.5 block truncate text-[11px] text-ink-3">
                                    <span className="text-ink-4">{item.category}</span>
                                    {item.subtitle ? (
                                      <span className="mx-1.5 text-ink-4">·</span>
                                    ) : null}
                                    {item.subtitle}
                                  </span>
                                </span>
                              </span>
                              <span className="shrink-0 flex items-center gap-1.5">
                                {pinnedMeta ? (
                                  <span className="inline-flex items-center gap-1 rounded-tezbar-chip border border-white/10 bg-white/[0.03] px-1.5 py-0.5 text-[10px] text-ink-3">
                                    <Kbd>⌥</Kbd>
                                    <Kbd>{pinnedSlotShortcutKey(pinnedMeta.slot)}</Kbd>
                                  </span>
                                ) : null}
                                {i === selectedSearch ? (
                                  <span className="text-[10px] font-mono text-ink-3">
                                    <Kbd>↵</Kbd>
                                  </span>
                                ) : null}
                              </span>
                            </>
                          )}
                        </button>
                      </li>
                    )
                  })}
                </GlideList>
              </div>
            </div>
          ) : null}

          {/* Answer stream */}
          {showAnswer ? (
            <div className="glass-card animate-tezbar-scale-in px-4 py-3">
              {!isStreaming && streamText ? (
                <div className="mb-2 flex justify-end">
                  <button
                    type="button"
                    className="rounded-tezbar-chip border border-white/10 bg-white/[0.03] px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-ink-3 transition hover:text-ink-2"
                    onClick={() => {
                      void speakAnswerText()
                    }}
                  >
                    Read aloud
                  </button>
                </div>
              ) : null}
              {isStreaming && !streamText ? (
                <p className="tezbar-thinking flex items-center gap-2 text-[12px] text-ink-3">
                  <span className="inline-flex gap-1">
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3" />
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3 [animation-delay:120ms]" />
                    <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-3 [animation-delay:240ms]" />
                  </span>
                  Thinking
                </p>
              ) : (
                <div className="max-h-48 overflow-y-auto">
                  {streamText ? (
                    <Suspense
                      fallback={
                        <p className="whitespace-pre-wrap text-[13.5px] leading-[1.55] text-ink-1">
                          {streamText}
                        </p>
                      }
                    >
                      <Markdown text={streamText} streaming={isStreaming} />
                    </Suspense>
                  ) : emptyAnswer ? (
                    <p className="text-[13.5px] leading-[1.55] text-ink-1">
                      No response from the selected provider. Check your provider settings and try
                      again.
                    </p>
                  ) : null}
                </div>
              )}
              {streamError ? (
                <p className="mt-2 text-[11.5px] text-rose-300" role="alert">
                  {streamError}
                </p>
              ) : null}
            </div>
          ) : null}

          {/* Inline status line */}
          {error || actionMsg ? (
            <div className="px-1">
              {error ? <Message tone="error">{error}</Message> : null}
              {actionMsg ? (
                actionMsgCard ? (
                  (() => {
                    const tone = nativeResultCardTone(actionMsgCard.kind)
                    const label = nativeResultCardLabel(actionMsgCard.kind)
                    // Multi-line output (info commands print several rows)
                    // gets a slightly taller, top-aligned card; single-line
                    // secrets/paths stay compact and centered.
                    const multiline = actionMsg.includes('\n')
                    const mono = actionMsgCard.kind === 'password' || actionMsgCard.kind === 'copied' || actionMsgCard.kind === 'info'
                    return (
                      <div
                        className={`flex ${multiline ? 'items-start' : 'items-center'} gap-2.5 rounded-lg border px-3 py-2.5 ${tone.card}`}
                        role="status"
                      >
                        <span
                          className={`flex h-7 w-7 ${multiline ? 'mt-0.5' : ''} shrink-0 items-center justify-center rounded-md border ${tone.chip}`}
                        >
                          <svg
                            width="16"
                            height="16"
                            viewBox="0 0 14 14"
                            fill="none"
                            stroke="currentColor"
                            strokeWidth="1.15"
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            aria-hidden
                          >
                            {actionMsgCard.iconKind ? (
                              <CommandIconGlyph kind={actionMsgCard.iconKind} />
                            ) : (
                              <KeyGlyph />
                            )}
                          </svg>
                        </span>
                        <span
                          className={`min-w-0 flex-1 select-all break-words ${mono ? 'font-mono' : ''} text-[12.5px] font-medium leading-relaxed tracking-wide ${tone.text}`}
                        >
                          {actionMsg}
                        </span>
                        {label ? (
                          <span
                            className={`shrink-0 ${multiline ? 'mt-1.5' : ''} text-[10px] font-medium uppercase tracking-wider ${tone.label}`}
                          >
                            {label}
                          </span>
                        ) : null}
                      </div>
                    )
                  })()
                ) : (
                  <Message>{actionMsg}</Message>
                )
              ) : null}
            </div>
          ) : null}
        </div>
      </div>

      {/* Footer hint bar — same glass-card shell as Clipboard / other views */}
      <div
        className={cx(
          'glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in',
          showSearchResults || showSuggestions || showDeepSearchLoading || showAnswer
            ? 'opacity-60'
            : ''
        )}
      >
        <div className="flex min-w-0 items-center justify-between gap-4">
          <HintBar className="min-w-0 flex-1">
            {terminalMode ? (
              <>
                <Hint
                  label="Stop"
                  keys={
                    <>
                      <Kbd>⌘</Kbd>
                      <Kbd>X</Kbd>
                    </>
                  }
                />
                <Hint
                  label="Delete"
                  keys={
                    <>
                      <Kbd>⌘</Kbd>
                      <Kbd>D</Kbd>
                    </>
                  }
                />
                <Hint
                  label="Pin / Unpin"
                  keys={
                    <>
                      <Kbd>⌘</Kbd>
                      <Kbd>P</Kbd>
                    </>
                  }
                />
                <Hint
                  label="Settings"
                  keys={
                    <>
                      <Kbd>⌘</Kbd>
                      <Kbd>K</Kbd>
                    </>
                  }
                />
              </>
            ) : isAiMode ? (
              <>
                <Hint label="Providers" keys={<Kbd>⌘,</Kbd>} />
                <Hint
                  label="Attach screen"
                  keys={
                    <>
                      <Kbd>⌘</Kbd>
                      <Kbd>⇧</Kbd>
                      <Kbd>S</Kbd>
                    </>
                  }
                />
                <Hint label="Open chat" keys={<Kbd>↵</Kbd>} />
                <Hint
                  label="New chat"
                  keys={
                    <>
                      <Kbd>⌘</Kbd>
                      <Kbd>N</Kbd>
                    </>
                  }
                />
                <Hint
                  label="Close window"
                  keys={
                    <>
                      <Kbd>Esc</Kbd>
                      <Kbd>⌘</Kbd>
                    </>
                  }
                />
              </>
            ) : (
              <>
                {isApplicationInput ? (
                  <Hint label="Open" keys={<Kbd>↵</Kbd>} />
                ) : isSlashInput ? (
                  <>
                    <Hint label="Complete" keys={<Kbd>↵</Kbd>} />
                    <Hint
                      label="Open"
                      keys={
                        <>
                          <Kbd>⌘</Kbd>
                          <Kbd>↵</Kbd>
                        </>
                      }
                    />
                  </>
                ) : (
                  <>
                    <Hint
                      label="Pin / Unpin"
                      keys={
                        <>
                          <Kbd>⌘</Kbd>
                          <Kbd>P</Kbd>
                        </>
                      }
                    />
                    <Hint
                      label="Save note"
                      keys={
                        <>
                          <Kbd>⌘</Kbd>
                          <Kbd>N</Kbd>
                        </>
                      }
                    />
                    {isDeepSearchMode && showSearchResults ? (
                      <Hint
                        label={searchResultNavigationActive ? 'Search input' : 'Results'}
                        keys={<Kbd>Tab</Kbd>}
                      />
                    ) : null}
                    {canQuickLookSelectedResult ? (
                      <Hint label="Quick Look" keys={<Kbd>Space</Kbd>} />
                    ) : null}
                  </>
                )}
                {!isCompletionInput ? (
                  <button
                    type="button"
                    aria-label="Use Deep Search"
                    aria-pressed={isDeepSearchMode}
                    className={cx(
                      'no-drag ml-auto inline-flex shrink-0 items-center gap-1.5 rounded-tezbar-chip text-[10.5px] transition focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cyan-300/50',
                      isDeepSearchMode ? 'text-cyan-200' : 'text-ink-3 hover:text-cyan-200'
                    )}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={activateDeepSearch}
                    title="Use Deep Search (Command + Enter)"
                  >
                    <span className="inline-flex items-center gap-0.5">
                      <Kbd>⌘</Kbd>
                      <Kbd>↵</Kbd>
                    </span>
                    <span>Deep Search</span>
                  </button>
                ) : null}
              </>
            )}
          </HintBar>
          <BackgroundTaskStatus
            onOpenIndexing={onOpenIndexingPage}
            onOpenExtensionRuntime={onOpenExtensionRuntime}
          />
        </div>
      </div>

      {pinnedCommandTooltip
        ? createPortal(
            <div
              id="pinned-command-tooltip"
              role="tooltip"
              style={{ left: pinnedCommandTooltip.left, top: pinnedCommandTooltip.top }}
              className="pointer-events-none fixed z-[100] max-w-[260px] -translate-y-1/2 rounded-[10px] border border-white/[0.12] bg-[#11141c]/95 px-3 py-2 shadow-[0_12px_36px_rgba(0,0,0,0.48)] backdrop-blur-xl"
            >
              <span
                aria-hidden
                className="absolute -left-[5px] top-1/2 h-[9px] w-[9px] -translate-y-1/2 rotate-45 border-b border-l border-white/[0.12] bg-[#11141c]"
              />
              <span className="block truncate text-[12px] font-semibold leading-4 text-ink-1">
                {pinnedCommandTooltip.title}
              </span>
              <span className="mt-0.5 block truncate text-[10px] leading-4 text-ink-3">
                {pinnedCommandTooltip.subtitle} · ⌥{pinnedCommandTooltip.shortcut}
              </span>
            </div>,
            document.body
          )
        : null}

      {terminalSettingsOpen ? (
        <div
          className="absolute inset-0 z-50 flex items-center justify-center bg-black/35 p-6 backdrop-blur-sm"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setTerminalSettingsOpen(false)
          }}
        >
          <form
            className="w-full max-w-[440px] rounded-[18px] border border-white/[0.1] bg-[#10131d]/96 p-4 shadow-[0_24px_80px_rgba(0,0,0,0.5)]"
            role="dialog"
            aria-modal="true"
            aria-labelledby="terminal-defaults-title"
            onSubmit={(event) => {
              event.preventDefault()
              saveTerminalSettings()
            }}
          >
            <div className="mb-4 flex items-start justify-between gap-4">
              <div>
                <h2 id="terminal-defaults-title" className="text-[14px] font-semibold text-ink-1">
                  Terminal defaults
                </h2>
                <p className="mt-1 text-[11px] leading-4 text-ink-4">
                  New sessions use these defaults. A long-running service can stay alive until you
                  stop it.
                </p>
              </div>
              <Kbd>Esc</Kbd>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <label className="block" htmlFor="terminal-default-save-for">
                <span className="mb-1.5 block text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-3">
                  Keep session history
                </span>
                <SelectField
                  id="terminal-default-save-for"
                  value={terminalSettingsDraft.saveFor}
                  onChange={(event) =>
                    setTerminalSettingsDraft((current) => ({
                      ...current,
                      saveFor: event.target.value as TerminalSaveFor,
                    }))
                  }
                >
                  <option value="day">1 day</option>
                  <option value="week">1 week</option>
                  <option value="month">1 month</option>
                  <option value="forever">Forever</option>
                </SelectField>
              </label>
              <label className="block" htmlFor="terminal-default-keep-alive">
                <span className="mb-1.5 block text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-3">
                  Keep process running
                </span>
                <SelectField
                  id="terminal-default-keep-alive"
                  value={terminalSettingsDraft.keepAliveFor}
                  onChange={(event) =>
                    setTerminalSettingsDraft((current) => ({
                      ...current,
                      keepAliveFor: event.target.value as TerminalKeepAliveFor,
                    }))
                  }
                >
                  <option value="3h">3 hours</option>
                  <option value="8h">8 hours</option>
                  <option value="day">1 day</option>
                  <option value="until-stop">Until stopped</option>
                </SelectField>
              </label>
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                className="rounded-tezbar-chip border border-white/10 px-3 py-1.5 text-[11px] font-semibold text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1"
                onClick={() => setTerminalSettingsOpen(false)}
              >
                Cancel
              </button>
              <button
                type="submit"
                className="rounded-tezbar-chip border border-emerald-300/35 bg-emerald-400/15 px-3 py-1.5 text-[11px] font-semibold text-emerald-100 transition hover:bg-emerald-400/20"
              >
                Save defaults
              </button>
            </div>
          </form>
        </div>
      ) : null}
    </div>
  )
}
