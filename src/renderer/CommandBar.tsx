import { type DragEvent, type FormEvent, useEffect, useMemo, useRef, useState } from 'react'
import type { Intent } from '../shared/intent'
import {
  defaultModels,
  normalizeProviderModelList,
} from '../shared/aiProviders'
import type { LlmConfigRecord, ProviderId } from '../shared/llmConfig'
import type { PathCompletionItem, SearchResult } from '../shared/search'
import type { ExtensionRunCommandResult } from '../shared/extensionRuntime'
import { Hint, HintBar, Kbd, Message, SelectField, TextField, cx } from './ui/primitives'
import { setCommandSurfaceEscapeConsumer } from './escapeGate'
import { GlideList } from './ui/GlideList'
import { Markdown } from './ui/Markdown'
import { RollingText } from './ui/RollingText'
import { useHoldToSpeak } from './hooks/useHoldToSpeak'
import { evaluateExpression, type CalcResult } from './calculator'
import { buildColorConversionResults } from './colorConverter'
import { parseCurrencyQuery } from './currency/parseCurrencyQuery'
import type { ChatSessionSummary } from '../shared/chat'
import type { AiChatBoot } from '../shared/aiChatSurface'
import { RAYMES_QUICK_NOTE_SHORTCUT_EVENT } from '../shared/aiChatSurface'
import type { TerminalPromptInfo } from '../shared/terminal'
import { getPreferredDefaultTarget } from './currency/currencyPreferences'
import { useCurrencyConversion } from './hooks/useCurrencyConversion'
import { ModelPicker } from './ModelPicker'

const RECENT_EXTENSION_COMMANDS_KEY = 'tezbar:recent-extension-commands'
const RECENT_EXTENSION_COMMANDS_LIMIT = 20
const PINNED_COMMANDS_KEY = 'tezbar:pinned-commands'
const MAX_PINNED_COMMANDS = 9
const COMMAND_HINTS = [
  { shortcut: '/directory', label: 'Search files and folders' },
  { shortcut: '`', label: 'Browse applications' },
  { shortcut: 'SPACE', label: 'Enter AI Space' },
  { shortcut: '>', label: 'Open terminal' },
] as const
const PIN_ICON_CHOICES = [
  '📌',
  '⭐',
  '🔥',
  '⚡',
  '🧠',
  '🛠️',
  '🚀',
  '🎯',
  '🧩',
  '📎',
  '🗂️',
  '🔧',
  '💡',
  '🧭',
  '🔒',
  '🧪',
  '🖥️',
  '📦',
  '📝',
  '🔖',
  '📁',
  '🧰',
  '🕹️',
  '🔍',
] as const

type PinIcon = (typeof PIN_ICON_CHOICES)[number]

type PinnedCommand = {
  id: string
  title: string
  subtitle: string
  category: SearchResult['category']
  action: SearchResult['action']
  icon: PinIcon
  /** ⌥+digit hotkey; unique among pins, 1–9 */
  slot: number
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
    JSON.stringify(next.slice(0, RECENT_EXTENSION_COMMANDS_LIMIT)),
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
        const icon = String(item?.icon ?? '') as PinIcon

        const hasValidIcon = PIN_ICON_CHOICES.includes(icon)
        const hasValidAction =
          typeof action === 'object' && action !== null && typeof (action as { type?: unknown }).type === 'string'

        if (!id || !title || typeof category !== 'string' || !hasValidAction || !hasValidIcon) {
          return null
        }

        const rawSlot = item?.slot
        const slotNum =
          typeof rawSlot === 'number' && rawSlot >= 1 && rawSlot <= 9 ? Math.floor(rawSlot) : undefined

        return {
          id,
          title,
          subtitle,
          category: category as SearchResult['category'],
          action: action as SearchResult['action'],
          icon,
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

function writePinnedCommands(next: PinnedCommand[]): void {
  window.localStorage.setItem(PINNED_COMMANDS_KEY, JSON.stringify(next.slice(0, MAX_PINNED_COMMANDS)))
}

function parseDigitIndex(key: string): number | null {
  if (!/^[1-9]$/.test(key)) return null
  return Number(key) - 1
}

const PIN_DRAG_MIME = 'application/x-tezbar-pin-id'

function parsePinnedSlotFromKeyEvent(event: KeyboardEvent): number | null {
  const fromCode = /^Digit([1-9])$/.exec(event.code)?.[1]
  if (fromCode) return Number(fromCode)
  if (/^[1-9]$/.test(event.key)) return Number(event.key)
  return null
}

type PinnedCommandDraft = Omit<PinnedCommand, 'slot'> & { slot?: number }

/** Ensure every pin has a valid unique slot in 1…9 (stable order). */
function normalizePinnedSlots(pins: PinnedCommandDraft[]): PinnedCommand[] {
  if (pins.length === 0) return []
  const claimed = new Set<number>()
  const first = pins.map((pin) => {
    const raw = pin.slot
    const n = typeof raw === 'number' && raw >= 1 && raw <= 9 ? Math.floor(raw) : null
    if (n !== null && !claimed.has(n)) {
      claimed.add(n)
      return { ...pin, slot: n } satisfies PinnedCommand
    }
    return { ...pin, slot: -1 }
  })
  for (const pin of first) {
    if (pin.slot !== -1) continue
    for (let d = 1; d <= 9; d++) {
      if (!claimed.has(d)) {
        pin.slot = d
        claimed.add(d)
        break
      }
    }
  }
  return first as PinnedCommand[]
}

function nextFreePinSlot(pins: PinnedCommand[]): number {
  const used = new Set(pins.map((p) => p.slot))
  for (let d = 1; d <= 9; d++) {
    if (!used.has(d)) return d
  }
  return 1
}

function reorderPinnedByDrop(pins: PinnedCommand[], draggedId: string, targetId: string): PinnedCommand[] {
  const from = pins.findIndex((p) => p.id === draggedId)
  const to = pins.findIndex((p) => p.id === targetId)
  if (from < 0 || to < 0 || from === to) return pins
  const next = [...pins]
  const [item] = next.splice(from, 1)
  if (!item) return pins
  const toAdj = from < to ? to - 1 : to
  next.splice(toAdj, 0, item)
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
      <path d="M4 5.5L6.5 7L4 8.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M8 9.5H10.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
    </svg>
  )
}

function pathCompletionSectionLabel(section: PathCompletionItem['section']): string | null {
  if (section === 'recommended') return 'Recommended'
  if (section === 'default') return 'Default'
  if (section === 'applications') return 'All Applications'
  return null
}

export default function CommandBar({
  initialValue = '',
  initialSelectedChatId = null,
  onOpenAiChat,
  onOpenSettings,
  onConfigureAi,
  onOpenExtensions,
  onOpenExtensionRuntime,
  onOpenPortsPage,
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
  onConfigureAi: () => void
  onOpenExtensions: () => void
  onOpenExtensionRuntime: (initial: ExtensionRuntimeViewPayload) => void
  onOpenPortsPage: (opts?: { tab?: 'listen' | 'named' }) => void
  onOpenClipboardPage: () => void
  onOpenSnippetsPage: () => void
  onOpenNotesPage: (opts?: { createdAt?: number }) => void
  onOpenEmojiPicker: () => void
  onOpenTerminal: (initialCommand?: string) => void
}): JSX.Element {
  const [value, setValue] = useState(initialValue)
  const [lastIntent, setLastIntent] = useState<Intent | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [streamText, setStreamText] = useState('')
  const [streamError, setStreamError] = useState<string | null>(null)
  const [isStreaming, setIsStreaming] = useState(false)
  const [cfg, setCfg] = useState<LlmConfigRecord>({})
  const [modelMenuOpen, setModelMenuOpen] = useState(false)
  const [emptyAnswer, setEmptyAnswer] = useState(false)
  const [pathCompletions, setPathCompletions] = useState<PathCompletionItem[]>([])
  const [recentExtensionCommands, setRecentExtensionCommands] = useState<string[]>([])
  const [pinnedCommands, setPinnedCommands] = useState<PinnedCommand[]>([])
  const [chatHistory, setChatHistory] = useState<ChatSessionSummary[]>([])
  const [draggingPinId, setDraggingPinId] = useState<string | null>(null)
  const [pinPickerTarget, setPinPickerTarget] = useState<SearchResult | null>(null)
  const [pinPickerIconIndex, setPinPickerIconIndex] = useState(0)
  const [selectedSuggestion, setSelectedSuggestion] = useState(0)
  const [followSuggestionSelection, setFollowSuggestionSelection] = useState(false)
  const [searchResults, setSearchResults] = useState<SearchResult[]>([])
  const [selectedSearch, setSelectedSearch] = useState(
    initialValue.startsWith(' ') || initialValue.endsWith('  ') ? -1 : 0
  )
  const [followSearchSelection, setFollowSearchSelection] = useState(false)
  const [actionMsg, setActionMsg] = useState<string | null>(null)
  const actionMsgTimeoutRef = useRef<NodeJS.Timeout | null>(null)

  const showActionMsg = (msg: string | null): void => {
    if (actionMsgTimeoutRef.current) {
      clearTimeout(actionMsgTimeoutRef.current)
      actionMsgTimeoutRef.current = null
    }
    setActionMsg(msg)
    if (msg) {
      actionMsgTimeoutRef.current = setTimeout(() => {
        setActionMsg(null)
        actionMsgTimeoutRef.current = null
      }, 4000)
    }
  }
  // Space prefix = AI mode in the launcher; the full chat UI lives on the
  // dedicated AI Chat surface (see App.tsx + AgentChatView).
  // Trigger AI mode if input starts with a space, or if it ends with exactly two spaces.
  const isAiMode = value.startsWith(' ') || value.endsWith('  ')
  const agentTask = isAiMode ? value.trim() : ''

  const [pendingAction, setPendingAction] = useState<
    | {
      extensionId: string
      commandName: string
      title: string
      commandArgumentDefinitions: PendingExtensionArgument[]
    }
    | null
  >(null)
  const [argumentValues, setArgumentValues] = useState<Record<string, string>>({})
  const [killPortMode, setKillPortMode] = useState(false)
  const [killPortQuery, setKillPortQuery] = useState('')
  const [killPortValue, setKillPortValue] = useState('')
  const [terminalMode, setTerminalMode] = useState(false)
  const [terminalPrompt, setTerminalPrompt] = useState('')
  const argInputRefs = useRef<Array<HTMLInputElement | HTMLSelectElement | null>>([])
  const gotAnyTokenRef = useRef(false)
  const pinPickerOpenRef = useRef(false)
  const pendingOpenRef = useRef(false)
  const modelMenuOpenRef = useRef(false)
  const valueRef = useRef(value)
  const killPortModeRef = useRef(killPortMode)
  const terminalModeRef = useRef(terminalMode)
  const lastSearchRequestId = useRef(0)

  useEffect(() => {
    valueRef.current = value
  }, [value])

  useEffect(() => {
    killPortModeRef.current = killPortMode
  }, [killPortMode])

  useEffect(() => {
    terminalModeRef.current = terminalMode
  }, [terminalMode])

  useEffect(() => {
    if (initialValue.startsWith('>')) {
      setTerminalMode(true)
      setValue(initialValue.slice(1))
    }
  }, [initialValue])

  useEffect(() => {
    if (!terminalMode) {
      setTerminalPrompt('')
      return
    }
    void window.tezbar.getTerminalPromptInfo().then((info: TerminalPromptInfo) => {
      if (info) {
        const prompt = `${info.user}@${info.host} ${info.dir} %`
        setTerminalPrompt(prompt)
      }
    })
  }, [terminalMode])

  useEffect(() => {
    setRecentExtensionCommands(readRecentExtensionCommands())
    setPinnedCommands(readPinnedCommands())
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
            setSearchResults(items)
            setSelectedSearch(0)
            setFollowSearchSelection(true)
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
  const isSlashInput = slashQuery.startsWith('/')
  const isApplicationInput = slashQuery.startsWith('`')
  const isCompletionInput = isSlashInput || isApplicationInput

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

  const shouldOfferKillPortCommand = useMemo(() => {
    const q = value.trim().toLowerCase()
    if (killPortMode) return true
    if (!/\bkill\b/.test(q) || !/\bport\b/.test(q)) return false
    return true
  }, [killPortMode, value])

  const killPortCommandResult = useMemo<SearchResult | null>(() => {
    if (!shouldOfferKillPortCommand) return null
    const port = killPortValue.trim()
    return {
      id: 'extcmd:raycast.port-manager:kill-listening-process',
      title: 'Kill Process Listening On',
      subtitle: 'Port Manager',
      category: 'extensions',
      score: 20_000,
      action: {
        type: 'run-extension-command',
        extensionId: 'raycast.port-manager',
        commandName: 'kill-listening-process',
        title: 'Kill Process Listening On',
        argumentValues: port ? { port } : undefined,
        commandArgumentDefinitions: [
          { name: 'port', title: 'Port', placeholder: 'Port', required: true, type: 'text' },
        ],
      },
    }
  }, [killPortValue, shouldOfferKillPortCommand])

  const visibleSearchResults = useMemo(() => {
    if (killPortMode) {
      const killProcessResult = searchResults.find((item) => item.id === 'extcmd:raycast.kill-process:index')
      return [
        ...(killPortCommandResult ? [killPortCommandResult] : []),
        ...(killProcessResult ? [killProcessResult] : []),
      ]
    }
    const withoutDuplicatePort = killPortCommandResult
      ? searchResults.filter((item) => item.id !== 'extcmd:raycast.port-manager:kill-listening-process')
      : searchResults
    const withoutDuplicateColorRows = colorConversionRows.length > 0
      ? withoutDuplicatePort.filter((item) => item.category !== 'color-converter')
      : withoutDuplicatePort
    const base = [
      ...(killPortCommandResult ? [killPortCommandResult] : []),
      ...colorConversionRows,
      ...withoutDuplicateColorRows,
    ]
    return calcResultRow ? [calcResultRow, ...base] : base
  }, [calcResultRow, colorConversionRows, killPortCommandResult, killPortMode, searchResults])
  const visibleSearchCount = visibleSearchResults.length
  const topResult = visibleSearchResults[0] ?? null
  const canEnterKillPortMode =
    !killPortMode &&
    !pendingAction &&
    !isCompletionInput &&
    !isAiMode &&
    !terminalMode &&
    topResult?.id === 'extcmd:raycast.kill-process:index'
  const pinnedMetaById = useMemo(() => {
    const out = new Map<string, { slot: number; icon: PinIcon }>()
    pinnedCommands.forEach((pin) => {
      out.set(pin.id, { slot: pin.slot, icon: pin.icon })
    })
    return out
  }, [pinnedCommands])

  const suggestions = useMemo(
    () => (isCompletionInput ? pathCompletions : []),
    [isCompletionInput, pathCompletions],
  )

  useEffect(() => {
    if (!isCompletionInput) {
      setPathCompletions([])
      return
    }

    let cancelled = false
    const t = setTimeout(() => {
      void window.tezbar.completePath(value).then((items) => {
        if (!cancelled) setPathCompletions(items)
      })
    }, 45)
    return () => {
      cancelled = true
      clearTimeout(t)
    }
  }, [isCompletionInput, value])

  useEffect(() => {
    let cancelled = false
    const requestId = ++lastSearchRequestId.current
    const t = setTimeout(() => {
      if (isAiMode || isCompletionInput || terminalMode) {
        setSearchResults([])
        return
      }
      void window.tezbar.searchAll(value).then((items) => {
        if (!cancelled && requestId === lastSearchRequestId.current) {
          setSearchResults(items)
        }
      })
    }, 120)
    return () => {
      cancelled = true
      clearTimeout(t)
    }
  }, [isAiMode, isCompletionInput, terminalMode, value])

  useEffect(() => {
    if (isCompletionInput || searchResults.length === 0 || recentExtensionCommands.length === 0) return
    // When the calc row is present it owns index 0 and should stay
    // selected — typing `2+2` should not jump to a recent app.
    if (calcResultRow) return
    const mostRecent = recentExtensionCommands[0]
    const idx = visibleSearchResults.findIndex((item) => item.id === mostRecent)
    if (idx >= 0 && idx < visibleSearchCount) {
      setSelectedSearch(idx)
    }
  }, [isCompletionInput, recentExtensionCommands, searchResults, visibleSearchResults, visibleSearchCount, calcResultRow])

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
    const firstAppIndex = suggestions.findIndex(
      (item) => item.kind === 'application' && item.applicationAction === 'open-with' && item.section !== 'default',
    )
    if (firstAppIndex >= 0) {
      setSelectedSuggestion(firstAppIndex)
      return
    }
    setSelectedSuggestion((i) => Math.min(Math.max(-1, i), suggestions.length - 1))
  }, [suggestions])

  const trackExtensionCommand = (extensionId: string, commandName: string): void => {
    const id = buildRecentExtensionCommandId(extensionId, commandName)
    const next = [id, ...recentExtensionCommands.filter((v) => v !== id)].slice(0, RECENT_EXTENSION_COMMANDS_LIMIT)
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

  const openPinPicker = (result: SearchResult): void => {
    const alreadyPinned = pinnedCommands.some((pin) => pin.id === result.id)
    if (alreadyPinned) {
      showActionMsg('Already pinned. Press ⌘P to unpin.')
      return
    }

    setPinPickerTarget(result)
    setPinPickerIconIndex(0)
    showActionMsg('Choose an emoji, then press Enter to pin')
  }

  const confirmPin = (iconOverride?: PinIcon): void => {
    if (!pinPickerTarget) return

    if (pinnedCommands.some((pin) => pin.id === pinPickerTarget.id)) {
      setPinPickerTarget(null)
      showActionMsg('Already pinned. Press ⌘P to unpin.')
      focusCommandInput()
      return
    }

    const icon = iconOverride ?? PIN_ICON_CHOICES[pinPickerIconIndex]
    if (!icon) return

    const slot = nextFreePinSlot(pinnedCommands)
    const next: PinnedCommand[] = [
      {
        id: pinPickerTarget.id,
        title: pinPickerTarget.title,
        subtitle: pinPickerTarget.subtitle,
        category: pinPickerTarget.category,
        action: pinPickerTarget.action,
        icon,
        slot,
      },
      ...pinnedCommands.filter((pin) => pin.id !== pinPickerTarget.id),
    ].slice(0, MAX_PINNED_COMMANDS)

    persistPinnedCommands(next)
    setPinPickerTarget(null)
    showActionMsg(`Pinned: ${pinPickerTarget.title}`)
    focusCommandInput()
  }

  const runPinnedCommand = async (pin: PinnedCommand, listIndex: number): Promise<void> => {
    const pinnedResult: SearchResult = {
      id: pin.id,
      title: pin.title,
      subtitle: pin.subtitle,
      category: pin.category,
      score: 1000 - listIndex,
      action: pin.action,
    }
    await runSelectedSearchResult(pinnedResult, listIndex + 1)
  }

  const cyclePinShortcutSlot = (pinId: string): void => {
    const pin = pinnedCommands.find((p) => p.id === pinId)
    if (!pin) return
    const taken = new Set(pinnedCommands.filter((p) => p.id !== pinId).map((p) => p.slot))
    let d = pin.slot
    for (let step = 0; step < 9; step++) {
      d = d >= 9 ? 1 : d + 1
      if (!taken.has(d)) {
        persistPinnedCommands(pinnedCommands.map((p) => (p.id === pinId ? { ...p, slot: d } : p)))
        showActionMsg(`Pinned shortcut: ⌥${d}`)
        return
      }
    }
  }

  const isDictating = holdToSpeak.state.kind === 'recording'
  const isTranscribing = holdToSpeak.state.kind === 'transcribing'
  const dictationSupported = holdToSpeak.supported
  const startDictation = holdToSpeak.press
  const stopDictation = holdToSpeak.release

  useEffect(() => {
    if (!dictationSupported) return undefined
    return window.tezbar.onVoiceHotkeyHold(({ phase }) => {
      if (phase === 'press') startDictation()
      else stopDictation()
    })
  }, [dictationSupported, startDictation, stopDictation])

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
  }

  const enterKillPortMode = (): void => {
    setKillPortQuery(value.trim() || 'kill process')
    setKillPortValue('')
    setKillPortMode(true)
    showActionMsg('Type a port, then press Enter')
    requestAnimationFrame(() => focusCommandInput())
  }

  const exitKillPortMode = (): void => {
    setKillPortMode(false)
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
      setKillPortMode(false)
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

    await executeExtensionCommandViaRuntime({
      extensionId: pendingAction.extensionId,
      commandName: pendingAction.commandName,
      argumentValues,
    })
  }

  async function runSelectedSearchResult(result: SearchResult, rank = selectedSearch + 1): Promise<void> {
    if (result.action.type === 'invoke-command') {
      clearPendingAction()
      showActionMsg(null)
      setValue('')

      if (result.action.commandId === 'open-providers') {
        onConfigureAi()
        return
      }
      if (result.action.commandId === 'open-settings') {
        onOpenSettings()
        return
      }
      if (result.action.commandId === 'open-extensions') {
        onOpenExtensions()
        return
      }
      if (result.action.commandId === 'open-snippets') {
        onOpenSnippetsPage()
        return
      }
      if (result.action.commandId === 'open-notes') {
        onOpenNotesPage()
        return
      }
      if (result.action.commandId === 'open-emoji-picker') {
        onOpenEmojiPicker()
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
        onOpenNotesPage({ createdAt })
        return
      }
    }

    if (
      result.action.type === 'run-extension-command' &&
      result.action.extensionId === 'raycast.port-manager' &&
      (result.action.commandName === 'open-ports' || result.action.commandName === 'open-ports-menu-bar')
    ) {
      clearPendingAction()
      showActionMsg(null)
      setValue('')
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
      onOpenPortsPage()
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
      onOpenEmojiPicker()
      return
    }

    if (
      result.action.type === 'run-native-command' &&
      result.action.commandId === 'quit-tezbar'
    ) {
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
        setKillPortValue('')
        setKillPortMode(true)
        showActionMsg('Type a port, then press Enter')
        requestAnimationFrame(() => focusCommandInput())
        return
      }

      const defs =
        Array.isArray(result.action.commandArgumentDefinitions) && result.action.commandArgumentDefinitions.length > 0
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
          {} as Record<string, string>,
        )

        setPendingAction({
          extensionId: result.action.extensionId,
          commandName: result.action.commandName,
          title: result.action.title,
          commandArgumentDefinitions: defs,
        })
        setArgumentValues(initialValues)
        showActionMsg('Fill arguments · Enter to run · Esc to cancel')
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
      showActionMsg(r.message)
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

  async function openPathCompletion(item: PathCompletionItem): Promise<void> {
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
    setValue(item.value)
    setSelectedSuggestion(0)
    requestAnimationFrame(() => focusCommandInput())
  }

  pinPickerOpenRef.current = pinPickerTarget !== null
  pendingOpenRef.current = pendingAction !== null
  modelMenuOpenRef.current = modelMenuOpen

  useEffect(() => {
    setCommandSurfaceEscapeConsumer(() => {
      if (modelMenuOpenRef.current) {
        setModelMenuOpen(false)
        focusCommandInput()
        return true
      }
      if (pinPickerOpenRef.current) {
        setPinPickerTarget(null)
        showActionMsg(null)
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
        setKillPortMode(false)
        setKillPortValue('')
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
      if (terminalModeRef.current) {
        setValue('')
        setTerminalMode(false)
        setTerminalPrompt('')
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
    requestAnimationFrame(() => argInputRefs.current[0]?.focus())
  }, [pendingAction])

  useEffect(() => {
    const onGlobalKeyDown = (event: KeyboardEvent): void => {
      if (pinPickerTarget) {
        if (event.key === 'Escape') {
          event.preventDefault()
          setPinPickerTarget(null)
          showActionMsg(null)
          focusCommandInput()
          return
        }

        if (event.key === 'Tab') {
          event.preventDefault()
          // Tab and Shift+Tab both cancel — same as Esc, and matches the hint copy.
          setPinPickerTarget(null)
          showActionMsg(null)
          focusCommandInput()
          return
        }

        const cols = 8
        if (event.key === 'ArrowRight') {
          event.preventDefault()
          setPinPickerIconIndex((i) => Math.min(i + 1, PIN_ICON_CHOICES.length - 1))
        } else if (event.key === 'ArrowLeft') {
          event.preventDefault()
          setPinPickerIconIndex((i) => Math.max(i - 1, 0))
        } else if (event.key === 'ArrowDown') {
          event.preventDefault()
          setPinPickerIconIndex((i) => Math.min(i + cols, PIN_ICON_CHOICES.length - 1))
        } else if (event.key === 'ArrowUp') {
          event.preventDefault()
          setPinPickerIconIndex((i) => Math.max(i - cols, 0))
        }

        const iconDigit = parseDigitIndex(event.key)
        if (iconDigit !== null && iconDigit < PIN_ICON_CHOICES.length) {
          event.preventDefault()
          const icon = PIN_ICON_CHOICES[iconDigit]
          if (!icon) return
          confirmPin(icon)
          return
        }

        if (event.key === 'Enter') {
          event.preventDefault()
          confirmPin()
          return
        }

        return
      }

      const hasCommandMod = event.metaKey || event.ctrlKey
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
            unpinCommandById(selected.id)
          } else {
            openPinPicker(selected)
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
            if (pin) void runPinnedCommand(pin, pinIndex)
          }
        }
      }
    }

    window.addEventListener('keydown', onGlobalKeyDown)
    return () => window.removeEventListener('keydown', onGlobalKeyDown)
  }, [
    confirmPin,
    isAiMode,
    openPinPicker,
    pinPickerIconIndex,
    pinPickerTarget,
    pinnedCommands,
    runPinnedCommand,
    selectedSearch,
    unpinCommandById,
    visibleSearchResults,
  ])

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
      const pendingFieldFocused = argInputRefs.current.some((el) => el === document.activeElement)
      if (pendingFieldFocused) {
        await submitPendingAction()
        return
      }
      clearPendingAction()
      showActionMsg(null)
    }

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
      onOpenAiChat({ kind: 'submit', prompt: task })
      setValue('  ')
      return
    }

    if (terminalMode) {
      const initialCommand = value.trim() || undefined
      onOpenTerminal(initialCommand)
      setValue('')
      setTerminalMode(false)
      setTerminalPrompt('')
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
        { query: value.trim(), resultId: `path-direct:${value.trim()}` },
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
      setLastIntent(intent)
    } catch (err) {
      setIsStreaming(false)
      setLastIntent(null)
      setError(err instanceof Error ? err.message : 'Query failed')
    }
  }

  // Don't stack the LLM answer card while the user is in AI mode (agent
  // chat opens on its own surface).
  const showAnswer =
    !isAiMode && (isStreaming || Boolean(streamText) || Boolean(streamError) || emptyAnswer)
  const showSuggestions = isCompletionInput && suggestions.length > 0
  const showSearchResults =
    !isCompletionInput && !isAiMode && !terminalMode && visibleSearchCount > 0

  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>): void => {
    if (pinPickerTarget) {
      // While pin picker is open, global key handling owns navigation.
      return
    }

    if (killPortMode) {
      if (e.key === 'Escape') {
        e.preventDefault()
        exitKillPortMode()
        return
      }
      if (e.key === 'Tab') {
        e.preventDefault()
        exitKillPortMode()
        return
      }
      return
    }

    if (terminalMode && e.key === 'Backspace' && !value) {
      e.preventDefault()
      setTerminalMode(false)
      setTerminalPrompt('')
      return
    }

    if (e.key === '>' && !terminalMode && !value && !isAiMode && !killPortMode && !pendingAction && !pinPickerTarget) {
      e.preventDefault()
      setTerminalMode(true)
      return
    }

    if (isCompletionInput && suggestions.length) {
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
    } else if (visibleSearchCount) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setFollowSearchSelection(true)
        setSelectedSearch((i) => Math.min(i + 1, visibleSearchCount - 1))
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setFollowSearchSelection(true)
        setSelectedSearch((i) => Math.max(i - 1, 0))
      }
    }

    if (e.key === 'h' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      onOpenClipboardPage()
    }

    if (e.key === 'Tab') {
      e.preventDefault()
      if (pendingAction) {
        argInputRefs.current[0]?.focus()
        return
      }
      if (pinPickerTarget) {
        // Just let it stay there or switch logic. But globally we handle it.
        return
      }
      if (isCompletionInput) {
        const item = suggestions[selectedSuggestion]
        if (item) completePathInput(item)
        return
      }
      if (showChatHistory) {
        setFollowSearchSelection(true)
        setSelectedSearch((i) =>
          e.shiftKey ? Math.max(i - 1, -1) : Math.min(i + 1, filteredChatHistory.length - 1)
        )
        return
      }
      if (canEnterKillPortMode) {
        enterKillPortMode()
        return
      }
      // If we are in results, and hit tab, we go to emoji
      if (showSearchResults) {
        const selected = visibleSearchResults[selectedSearch] ?? visibleSearchResults[0]
        if (selected && selected.category !== 'calculator') {
          openPinPicker(selected)
        }
      }
    }
    // Enter with completion suggestions: let the form `onSubmit` run so
    // one keypress executes the highlighted file or application.
  }

  async function selectAiModel(nextProvider: ProviderId, nextModel: string): Promise<void> {
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
      provider: nextProvider,
      model: nextModel,
      providerModels,
      providerSelectedModels,
      taskProviderOverrides: { ...cfg.taskProviderOverrides, chat: nextProvider },
      taskModelOverrides: { ...cfg.taskModelOverrides, chat: nextModel },
    }
    setCfg((current) => ({ ...current, ...patch }))
    setModelMenuOpen(false)
    await window.tezbar.setLlmConfig(patch)
    const next = await window.tezbar.getLlmConfig()
    setCfg(next as LlmConfigRecord)
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-2">
      {/* Primary glass card: icon + input */}
      <div className="glass-card relative z-30 shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <form className="relative w-full" onSubmit={(ev) => void onSubmit(ev)}>
          <div className="flex items-center gap-3">
            <span className={cx(isAiMode ? 'text-violet-300' : terminalMode ? 'text-emerald-300' : 'text-ink-3')}>
              {isAiMode ? <AiIcon /> : terminalMode ? <TerminalIcon /> : <SearchIcon />}
            </span>
            {isAiMode ? (
              <span
                aria-label="AI mode"
                className="inline-flex shrink-0 items-center gap-1 rounded-tezbar-chip border border-violet-400/40 bg-violet-500/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-violet-200"
              >
                <span className="h-1.5 w-1.5 rounded-full bg-violet-300" />
                AI
              </span>
            ) : null}
            {terminalMode ? (
              <span
                aria-label="Terminal mode"
                className="inline-flex shrink-0 items-center gap-1 rounded-tezbar-chip border border-emerald-400/35 bg-emerald-500/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-emerald-200"
              >
                <span className="font-mono text-[11px] leading-none">&gt;_</span>
                Terminal
              </span>
            ) : null}
            {killPortMode ? (
              <>
                <span className="max-w-[220px] truncate font-display text-[15px] text-ink-1">
                  {killPortQuery}
                </span>
                <span
                  aria-label="Port mode"
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
              </>
            ) : null}
            <div className="relative min-w-0 flex-1 flex items-center">
              {terminalMode ? (
                <span className="shrink-0 font-mono text-[13px] text-emerald-300/80 mr-1 select-none pointer-events-none">
                  {terminalPrompt}
                </span>
              ) : null}
              {!killPortMode && !isAiMode && !terminalMode && !value ? <RollingText items={COMMAND_HINTS} /> : null}
              <input
                id="command-input"
                type="text"
                value={killPortMode ? killPortValue : value}
                onChange={(e) => {
                  if (killPortMode) {
                    setKillPortValue(e.target.value.replace(/[^\d]/g, '').slice(0, 5))
                    return
                  }
                  if (pendingAction) {
                    clearPendingAction()
                    showActionMsg(null)
                  }
                  let newValue = e.target.value
                  if (!terminalMode && newValue.startsWith('>')) {
                    setTerminalMode(true)
                    newValue = newValue.slice(1)
                  }
                  setValue(newValue)
                  setSelectedSuggestion(0)
                  setSelectedSearch(
                    newValue.startsWith(' ') || newValue.endsWith('  ') ? -1 : 0
                  )
                }}
                onKeyDown={handleInputKeyDown}
                aria-label="Search TezBar or use a shortcut"
                placeholder={
                  killPortMode
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
            {isAiMode ? (
              <ModelPicker
                config={cfg}
                open={modelMenuOpen}
                onOpenChange={setModelMenuOpen}
                onSelect={selectAiModel}
                onConfigure={onConfigureAi}
                triggerClassName="font-mono leading-none tabular-nums tracking-normal"
              />
            ) : null}
            {dictationSupported ? (
              <button
                type="button"
                className={cx(
                  'inline-flex h-6 min-w-[116px] shrink-0 items-center justify-center rounded-tezbar-chip border px-2 text-[10px] font-medium uppercase leading-none tracking-[0.12em] transition',
                  isDictating
                    ? 'border-rose-400/40 bg-rose-500/20 text-rose-200'
                    : isTranscribing
                      ? 'border-amber-400/40 bg-amber-500/15 text-amber-200'
                      : 'border-white/10 bg-white/[0.03] text-ink-3 hover:text-ink-2',
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
                title="Hold to speak — or keep Option+Space held after opening TezBar (macOS)"
              >
                {isDictating ? 'Listening' : isTranscribing ? 'Transcribing…' : (
                  <span className="group">
                    <span className="group-hover:hidden">Hold to speak</span>
                    <span className="hidden group-hover:inline">hold cmd+space</span>
                  </span>
                )}
              </button>
            ) : null}
          </div>
        </form>
      </div>

      {/* Middle column: flex-1 so the search list can grow to the footer; inner
          panels scroll (GlideList, answer, …) instead of this outer region. */}
      <div className="flex min-h-0 flex-1 flex-col gap-[var(--s-2)] overflow-hidden pr-0.5">
        {/* Pinned commands */}
        {pinnedCommands.length > 0 && !isCompletionInput && !isAiMode ? (
          <div className="glass-card animate-tezbar-scale-in px-2 py-2">
            <div className="flex items-center gap-1.5 overflow-x-auto">
              {pinnedCommands.map((pin, index) => (
                <div
                  key={`pin:${pin.id}`}
                  draggable
                  title="Drag to reorder · Click icon to run · Click number to change shortcut · Right-click to unpin"
                  onDragStart={(e: DragEvent) => {
                    e.dataTransfer.setData(PIN_DRAG_MIME, pin.id)
                    e.dataTransfer.effectAllowed = 'move'
                    setDraggingPinId(pin.id)
                  }}
                  onDragEnd={() => {
                    setDraggingPinId(null)
                  }}
                  onDragOver={(e: DragEvent) => {
                    e.preventDefault()
                    e.dataTransfer.dropEffect = 'move'
                  }}
                  onDrop={(e: DragEvent) => {
                    e.preventDefault()
                    const fromId = e.dataTransfer.getData(PIN_DRAG_MIME)
                    if (!fromId || fromId === pin.id) return
                    persistPinnedCommands(reorderPinnedByDrop(pinnedCommands, fromId, pin.id))
                  }}
                  onContextMenu={(event) => {
                    event.preventDefault()
                    unpinCommandById(pin.id)
                  }}
                  className={cx(
                    'relative flex shrink-0 cursor-grab flex-col items-center gap-1 rounded-tezbar-row border border-white/10 bg-white/[0.03] px-1.5 py-1.5 transition active:cursor-grabbing',
                    draggingPinId === pin.id ? 'opacity-45' : 'hover:border-white/20 hover:bg-white/[0.07]',
                  )}
                >
                  <button
                    type="button"
                    draggable={false}
                    title={pin.title}
                    className="group grid h-7 w-7 shrink-0 place-items-center rounded-tezbar-chip border border-white/12 bg-white/[0.05] text-[14px] text-ink-1 transition hover:border-white/20 hover:bg-white/[0.08]"
                    onClick={() => {
                      void runPinnedCommand(pin, index)
                    }}
                  >
                    {pin.icon}
                  </button>
                  <button
                    type="button"
                    draggable={false}
                    title="Change ⌥ shortcut"
                    className="font-mono text-[9px] text-ink-4 transition hover:text-ink-2"
                    onClick={(event) => {
                      event.stopPropagation()
                      cyclePinShortcutSlot(pin.id)
                    }}
                  >
                    <Kbd>⌥</Kbd>
                    <Kbd>{pin.slot}</Kbd>
                  </button>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        {/* Pin icon picker */}
        {pinPickerTarget ? (
          <div className="glass-card animate-tezbar-scale-in px-3 py-2.5">
            <p className="text-[11px] font-semibold tracking-tight text-ink-2">
              Pin icon for <span className="text-ink-1">{pinPickerTarget.title}</span>
            </p>
            <div className="mt-2 grid grid-cols-8 gap-1">
              {PIN_ICON_CHOICES.map((icon, index) => (
                <button
                  key={`pin-icon:${icon}`}
                  type="button"
                  className={cx(
                    'grid h-8 w-full place-items-center rounded-tezbar-chip border text-[14px] transition',
                    PIN_ICON_CHOICES[pinPickerIconIndex] === icon
                      ? 'border-accent/60 bg-accent/15 text-ink-1'
                      : 'border-white/10 bg-white/[0.03] text-ink-2 hover:border-white/20 hover:text-ink-1',
                  )}
                  title={`Icon ${index + 1}`}
                  onClick={() => {
                    setPinPickerIconIndex(index)
                    confirmPin(icon)
                  }}
                >
                  {icon}
                </button>
              ))}
            </div>
            <p className="mt-2 text-[10.5px] text-ink-4">
              Pick an emoji, then press Enter to confirm. Esc or Tab to cancel.
            </p>
          </div>
        ) : null}

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
            <GlideList
              selectedIndex={selectedSuggestion}
              itemCount={suggestions.length}
              followSelected={followSuggestionSelection}
              className="min-h-0 flex-1 overflow-y-auto"
            >
              {suggestions.map((item, i) => {
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
                      onMouseEnter={() => {
                        setFollowSuggestionSelection(false)
                        setSelectedSuggestion(i)
                      }}
                      onMouseDown={(ev) => ev.preventDefault()}
                      onClick={() => completePathInput(item)}
                    >
                      <span className="flex min-w-0 flex-1 items-center gap-3">
                        {item.iconDataUrl ? (
                          <img
                            src={item.iconDataUrl}
                            alt=""
                            className="h-7 w-7 shrink-0 rounded-[7px]"
                            draggable={false}
                          />
                        ) : item.kind === 'application' ? (
                          <span className="grid h-7 w-7 shrink-0 place-items-center rounded-[7px] border border-white/10 bg-white/[0.04] text-[11px] font-semibold text-ink-3">
                            {item.title.slice(0, 1).toUpperCase()}
                          </span>
                        ) : null}
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
          </div>
        ) : null}

        {/* Pending extension action form */}
        {pendingAction ? (
          <form
            onSubmit={(ev) => {
              ev.preventDefault()
              void submitPendingAction()
            }}
            className="glass-card animate-tezbar-scale-in px-3 py-2.5"
            style={{
              boxShadow:
                'inset 0 1px 0 rgba(52, 211, 153, 0.12), inset 0 0 0 1px rgba(52, 211, 153, 0.25)',
            }}
          >
            <p className="mb-2 flex items-center gap-2 text-[11px] font-semibold tracking-tight text-emerald-300">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
              {pendingAction.title}
              <span className="text-ink-4">·</span>
              <span className="font-normal text-ink-3">
                {pendingAction.commandArgumentDefinitions.length} field
                {pendingAction.commandArgumentDefinitions.length === 1 ? '' : 's'}
              </span>
            </p>
            <div className="space-y-2">
              {pendingAction.commandArgumentDefinitions.map((arg, index) => {
                const fieldType = arg.type === 'dropdown' ? 'dropdown' : 'text'
                const label = arg.title || arg.name
                const placeholder = arg.placeholder || arg.title || arg.name
                const currentValue = argumentValues[arg.name] ?? ''

                const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement | HTMLSelectElement>): void => {
                  if (e.key === 'Escape') {
                    e.preventDefault()
                    cancelPendingAction()
                    return
                  }
                  if (e.key === 'Tab') {
                    e.preventDefault()
                    const nextIndex = e.shiftKey ? index - 1 : index + 1
                    if (nextIndex >= 0 && nextIndex < pendingAction.commandArgumentDefinitions.length) {
                      argInputRefs.current[nextIndex]?.focus()
                    } else {
                      focusCommandInput()
                    }
                    return
                  }
                  if (e.key === 'Enter') {
                    e.preventDefault()
                    void submitPendingAction()
                  }
                }

                return (
                  <div
                    key={`${pendingAction.commandName}:${arg.name}`}
                    className="flex items-center gap-3"
                  >
                    <span className="w-[88px] shrink-0 text-[11px] font-medium text-ink-3">
                      {label}
                      {arg.required ? <span className="text-emerald-400/80"> *</span> : null}
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
                        }}
                        onKeyDown={onKeyDown}
                        className="min-w-0 flex-1"
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
                        }}
                        onKeyDown={onKeyDown}
                        placeholder={placeholder}
                        autoComplete="off"
                        spellCheck={false}
                        className="min-w-0 flex-1"
                      />
                    )}
                  </div>
                )
              })}
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
              setSelectedSearch(-1)
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
                      onMouseEnter={() => {
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

        {/* Search results — grows to fill space below pinned / other chrome */}
        {showSearchResults ? (
          <div
            className="flex min-h-0 flex-1 flex-col"
            onWheelCapture={() => setFollowSearchSelection(false)}
            onMouseLeave={() => {
              setFollowSearchSelection(false)
              setSelectedSearch(-1)
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
                  const pinnedMeta = pinnedMetaById.get(item.id)
                  const isCalc = item.category === 'calculator'
                  const isColorConversion = item.category === 'color-converter'
                  const isCurrencyRow = isCalc && item.id.startsWith('currency:')
                  const colorSwatch =
                    isColorConversion && item.action.type === 'copy-text' ? item.action.text : item.title
                  return (
                    <li key={item.id} className="relative z-[1]">
                      <button
                        type="button"
                        className={cx(
                          'relative flex w-full items-center justify-between gap-3 rounded-tezbar-row text-left transition',
                          isCalc || isColorConversion ? 'px-3 py-2.5' : 'px-3 py-2',
                        )}
                        onMouseEnter={() => {
                          setFollowSearchSelection(false)
                          setSelectedSearch(i)
                        }}
                        onMouseDown={(ev) => ev.preventDefault()}
                        onClick={() => {
                          setSelectedSearch(i)
                          void runSelectedSearchResult(item, i + 1)
                        }}
                      >
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
                                    <rect x="4.25" y="3.25" width="5.5" height="2" rx="0.4" fill="currentColor" />
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
                                    {isColorConversion ? 'Color' : isCurrencyRow ? 'Currency' : 'Calculator'}
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
                            <span className="min-w-0 flex-1">
                              <span className="block truncate text-[13px] font-medium text-ink-1">{item.title}</span>
                              <span className="mt-0.5 block truncate text-[11px] text-ink-3">
                                <span className="text-ink-4">{item.category}</span>
                                {item.subtitle ? <span className="mx-1.5 text-ink-4">·</span> : null}
                                {item.subtitle}
                              </span>
                            </span>
                            <span className="shrink-0 flex items-center gap-1.5">
                              {pinnedMeta ? (
                                <span className="inline-flex items-center gap-1 rounded-tezbar-chip border border-amber-300/30 bg-amber-300/10 px-1.5 py-0.5 text-[10px] text-amber-100/95">
                                  <span className="text-[11px] leading-none">{pinnedMeta.icon}</span>
                                  <Kbd>⌥</Kbd>
                                  <Kbd>{pinnedMeta.slot}</Kbd>
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
                  <Markdown text={streamText} streaming={isStreaming} />
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

        {/* Inline status line (errors, intent, action msg) */}
        {error || lastIntent || actionMsg ? (
          <div className="px-1">
            {error ? <Message tone="error">{error}</Message> : null}
            {lastIntent && !error ? (
              <Message tone="info">
                {lastIntent.type}
                {lastIntent.type === 'extension' && 'name' in lastIntent ? ` · ${lastIntent.name}` : ''}
              </Message>
            ) : null}
            {actionMsg ? <Message>{actionMsg}</Message> : null}
          </div>
        ) : null}
      </div>

      {/* Footer hint bar — same glass-card shell as Clipboard / other views */}
      <div
        className={cx(
          'glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in',
          showSearchResults || showSuggestions || showAnswer ? 'opacity-60' : '',
        )}
      >
        <HintBar>
          {isAiMode ? (
            <>
              <Hint label="Providers" keys={<Kbd>⌘,</Kbd>} />
              <Hint label="Open chat" keys={<Kbd>↵</Kbd>} />
              <Hint label="New chat" keys={<><Kbd>⌘</Kbd><Kbd>N</Kbd></>} />
              <Hint label="Exit AI" keys={<Kbd>Esc</Kbd>} />
              <Hint label="Close window" keys={<><Kbd>Esc</Kbd><Kbd>⌘</Kbd></>} />
            </>
          ) : (
            <>
              {isApplicationInput ? (
                <Hint label="Open" keys={<Kbd>↵</Kbd>} />
              ) : isSlashInput ? (
                <>
                  <Hint label="Complete" keys={<Kbd>↵</Kbd>} />
                  <Hint label="Open" keys={<><Kbd>⌘</Kbd><Kbd>↵</Kbd></>} />
                </>
              ) : (
                <>
                  <Hint label="Pin / Unpin" keys={<><Kbd>⌘</Kbd><Kbd>P</Kbd></>} />
                  <Hint label="Pinned" keys={<><Kbd>⌥</Kbd><Kbd>1-9</Kbd></>} />
                  <Hint label="Save note" keys={<><Kbd>⌘</Kbd><Kbd>N</Kbd></>} />
                </>
              )}
              <Hint label="Navigate" keys={<><Kbd>↑</Kbd><Kbd>↓</Kbd></>} />
              {!isCompletionInput ? <Hint label="Run" keys={<Kbd>↵</Kbd>} /> : null}
              <Hint
                label={pinPickerTarget ? 'Cancel picker' : 'Close'}
                keys={
                  pinPickerTarget ? (
                    <>
                      <Kbd>Esc</Kbd>
                      <span className="text-ink-4">·</span>
                      <Kbd>Tab</Kbd>
                    </>
                  ) : (
                    <Kbd>Esc</Kbd>
                  )
                }
              />
            </>
          )}
        </HintBar>
      </div>
    </div>
  )
}
