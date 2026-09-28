import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import type { PermissionId, PermissionState, PermissionsSnapshot } from '../shared/permissions'
import type { KnowledgeSnapshot, KnowledgeStatus } from '../shared/knowledge'
import type { LlmConfigRecord, ProviderId } from '../shared/llmConfig'
import type { PathCompletionItem } from '../shared/search'
import {
  defaultBaseUrl,
  inferCapabilities,
  isAiProviderConfigured,
  normalizeProviderModelList,
  providerTitle,
  recommendedModel,
} from '../shared/aiProviders'
import { Button, Kbd, Message, cx } from './ui/primitives'
import { formatShortcutForDisplay, keyEventToAccelerator } from './HotkeyRecorder'
import { evaluateExpression } from './calculator'
import { commandBarInputMode } from './commandBarInputMode'
import appIconUrl from './assets/app-icon.png'

const IS_WINDOWS = navigator.platform.includes('Win')
const DEFAULT_HOTKEY = IS_WINDOWS ? 'Control+Space' : 'Alt+Space'
const MACHINE = IS_WINDOWS ? 'this PC' : 'this Mac'

/** Only the permissions with a real native OS prompt get a "Grant" button here.
 *  Automation, Input Monitoring, and Calendar have no direct probe/prompt on
 *  macOS and stay in Settings → Permissions where their deep-link flow lives. */
const FEATURED_PERMISSION_IDS: PermissionId[] = ['accessibility', 'microphone', 'screen-recording']

type StepId = 'shortcut' | 'practice' | 'ai' | 'deepsearch' | 'permissions'

type StepSummary = { done: boolean; detail: string }

const STEPS: { id: StepId; label: string }[] = [
  { id: 'shortcut', label: 'Shortcut' },
  { id: 'practice', label: 'Try the bar' },
  { id: 'ai', label: 'AI model' },
  { id: 'deepsearch', label: 'DeepSearch' },
  // Windows has no per-app permission prompts for any of these.
  ...(IS_WINDOWS ? [] : [{ id: 'permissions' as const, label: 'Permissions' }]),
]

const LAST_STEP = STEPS.length - 1
const SPLASH_AUTO_DISMISS_MS = 2200
const SPLASH_EXIT_MS = 420

/** Accelerator → individual key caps. macOS shows the modifier glyphs the
 *  rest of the app uses; Windows keeps the words people see on the keyboard. */
function shortcutKeys(accelerator: string): string[] {
  const parts = accelerator.split('+').map((part) => part.trim()).filter(Boolean)
  if (IS_WINDOWS) return parts.map((part) => (part === 'Control' ? 'Ctrl' : part))
  return parts.map((part) => formatShortcutForDisplay(part))
}

function shortcutLabel(accelerator: string): string {
  return shortcutKeys(accelerator).join(IS_WINDOWS ? '+' : ' ')
}

function tildify(path: string): string {
  const home = path.match(/^(\/Users\/[^/]+|\/home\/[^/]+|[A-Z]:\\Users\\[^\\]+)/)?.[1]
  return home ? `~${path.slice(home.length)}` : path
}

function isTypingTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement
}

/* =========================================================================
   Icons
   ========================================================================= */
function Glyph({ d, className, size = 14 }: { d: string; className?: string; size?: number }): JSX.Element {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden className={className}>
      <path d={d} />
    </svg>
  )
}

const ICON = {
  check: 'm5.5 12.5 4 4 9-9.5',
  folder: 'M3.5 6.5a1.5 1.5 0 0 1 1.5-1.5h4l2 2h8a1.5 1.5 0 0 1 1.5 1.5v8.5a1.5 1.5 0 0 1-1.5 1.5h-14A1.5 1.5 0 0 1 3.5 17Z',
  file: 'M7 3.5h7l4 4v12a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1ZM14 3.5v4h4',
  app: 'M4 4h6.5v6.5H4zM13.5 4H20v6.5h-6.5zM4 13.5h6.5V20H4zM13.5 13.5H20V20h-6.5z',
  search: 'M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13ZM19.5 19.5l-4.3-4.3',
  close: 'M6 6l12 12M18 6 6 18',
}

/* =========================================================================
   Step 1 — the global shortcut, recorded and saved for real
   ========================================================================= */
function ShortcutStep({
  hotkey,
  onHotkeyChange,
}: {
  hotkey: string
  onHotkeyChange: (hotkey: string) => void
}): JSX.Element {
  const [recording, setRecording] = useState(false)
  const [held, setHeld] = useState<string[]>([])
  const [saving, setSaving] = useState(false)
  const [message, setMessage] = useState<{ tone: 'success' | 'error'; text: string } | null>(null)

  const save = useCallback(
    async (accelerator: string) => {
      setSaving(true)
      setMessage(null)
      try {
        const result = await window.tezbar.setLlmConfig({ raymesHotkey: accelerator })
        if (result && typeof result === 'object' && 'ok' in result && !result.ok) {
          setMessage({
            tone: 'error',
            text: `${shortcutLabel(accelerator)} is taken by another app or by the system. Try a different combination.`,
          })
          return
        }
        onHotkeyChange(accelerator)
        setMessage({ tone: 'success', text: `Saved. ${shortcutLabel(accelerator)} now opens Tezbar.` })
      } catch {
        setMessage({ tone: 'error', text: "Couldn't save the shortcut. Try again, or change it later in Settings → General." })
      } finally {
        setSaving(false)
      }
    },
    [onHotkeyChange]
  )

  useEffect(() => {
    if (!recording) return
    const modifiers = (e: KeyboardEvent): string[] =>
      IS_WINDOWS
        ? [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win'].filter(Boolean) as string[]
        : [e.ctrlKey && '⌃', e.altKey && '⌥', e.shiftKey && '⇧', e.metaKey && '⌘'].filter(Boolean) as string[]
    const onDown = (e: KeyboardEvent): void => {
      e.preventDefault()
      e.stopImmediatePropagation()
      if (e.key === 'Escape') {
        setRecording(false)
        setHeld([])
        return
      }
      setHeld(modifiers(e))
      const accelerator = keyEventToAccelerator(e)
      if (!accelerator) return
      setRecording(false)
      setHeld([])
      void save(accelerator)
    }
    const onUp = (e: KeyboardEvent): void => {
      e.stopImmediatePropagation()
      setHeld(modifiers(e))
    }
    window.addEventListener('keydown', onDown, true)
    window.addEventListener('keyup', onUp, true)
    return () => {
      window.removeEventListener('keydown', onDown, true)
      window.removeEventListener('keyup', onUp, true)
    }
  }, [recording, save])

  const keys = recording ? held : shortcutKeys(hotkey)

  return (
    <>
      <StepHeading title="Pick the shortcut that opens Tezbar">
        Tezbar stays out of the way until you press this, from any app. Press it again, or Esc, to put
        it away.
      </StepHeading>

      <div
        className={cx(
          'onboarding-keystage mt-6 flex h-[148px] flex-col items-center justify-center gap-4 rounded-tezbar-panel border transition-colors',
          recording ? 'is-recording border-accent/40 bg-accent/[0.06]' : 'border-white/[0.07] bg-white/[0.02]'
        )}
      >
        <div className="flex h-[52px] items-center gap-2" aria-live="polite">
          {keys.length > 0 ? (
            keys.map((key, index) => (
              <span key={`${key}-${index}`} className="onboarding-keycap">
                {key}
              </span>
            ))
          ) : (
            <span className="text-[14px] text-ink-3">Press the keys together…</span>
          )}
        </div>
        <p className="text-[11.5px] text-ink-4">
          {recording
            ? 'Hold a modifier, then press a key. Esc cancels.'
            : saving
              ? 'Saving…'
              : 'Works in every app, even full-screen ones.'}
        </p>
      </div>

      <div className="mt-4 flex items-center gap-3">
        <Button variant={recording ? 'ghost' : 'primary'} onClick={() => setRecording((value) => !value)} disabled={saving}>
          {recording ? 'Cancel' : 'Change shortcut'}
        </Button>
        {hotkey !== DEFAULT_HOTKEY && !recording ? (
          <Button variant="quiet" onClick={() => void save(DEFAULT_HOTKEY)} disabled={saving}>
            Reset to {shortcutLabel(DEFAULT_HOTKEY)}
          </Button>
        ) : null}
      </div>
      <div className="mt-3 min-h-[18px]">{message ? <Message tone={message.tone}>{message.text}</Message> : null}</div>
    </>
  )
}

/* =========================================================================
   Step 2 — a working copy of the command bar's input rules
   ========================================================================= */
type PracticeTaskId = 'math' | 'folder' | 'terminal' | 'ai'

const PRACTICE_TASKS: { id: PracticeTaskId; trigger: string; title: string; hint: string; example: string }[] = [
  { id: 'math', trigger: '=', title: 'Do quick math', hint: 'Type a sum, like 12% of 480', example: '12% of 480' },
  { id: 'folder', trigger: '/', title: 'Jump to a folder', hint: 'Start with / and a folder name', example: '/' },
  { id: 'terminal', trigger: '>', title: 'Open a terminal', hint: 'Press > on an empty bar', example: '>' },
  { id: 'ai', trigger: 'Space', title: 'Ask the AI', hint: 'Start with a space, then ask', example: ' ' },
]

type BarMode = 'search' | 'ai' | 'terminal'

const AI_PUFF = ['#a5a6ff', '#c4b5fd', '#f0abfc', '#93c5fd']
const TERMINAL_PUFF = ['#34d399', '#6ee7b7', '#a7f3d0']

/** The same icons the real command bar swaps between (CommandBar.tsx). */
function BarIcon({ mode }: { mode: BarMode }): JSX.Element {
  if (mode === 'ai') {
    return (
      <svg width="16" height="16" viewBox="0 0 14 14" fill="none" aria-hidden>
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
  if (mode === 'terminal') {
    return (
      <svg width="16" height="16" viewBox="0 0 14 14" fill="none" aria-hidden>
        <rect x="1.5" y="1.5" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.3" />
        <path d="M4 5.5L6.5 7L4 8.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M8 9.5H10.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
      </svg>
    )
  }
  return <Glyph d={ICON.search} size={16} />
}

/** A small one-shot puff out of the bar's icon when the mode changes —
 *  violet for AI, green for the terminal. Remounted (via `key`) on every
 *  switch so it replays; nothing keeps moving afterwards. */
function PuffBurst({ palette }: { palette: readonly string[] }): JSX.Element {
  const puffs = useMemo(
    () =>
      Array.from({ length: 7 }, (_, index) => ({
        x: 6 + Math.random() * 90,
        y: (Math.random() - 0.5) * 22,
        size: 14 + Math.random() * 14,
        color: palette[index % palette.length],
        delay: Math.random() * 140,
      })),
    [palette]
  )
  return (
    <span className="onboarding-puff-burst" aria-hidden>
      {puffs.map((puff, index) => (
        <span
          key={index}
          className="onboarding-puff"
          style={
            {
              '--puff-x': `${puff.x}px`,
              '--puff-y': `${puff.y}px`,
              width: puff.size,
              height: puff.size,
              background: puff.color,
              animationDelay: `${puff.delay}ms`,
            } as CSSProperties
          }
        />
      ))}
    </span>
  )
}

function PracticeStep({
  done,
  onTaskDone,
  aiReady,
}: {
  done: ReadonlySet<PracticeTaskId>
  onTaskDone: (id: PracticeTaskId) => void
  aiReady: boolean
}): JSX.Element {
  const inputRef = useRef<HTMLInputElement>(null)
  const [value, setValue] = useState('')
  const [terminalMode, setTerminalMode] = useState(false)
  const [terminalDir, setTerminalDir] = useState<string | undefined>()
  const [completions, setCompletions] = useState<{ query: string; items: PathCompletionItem[] } | null>(null)
  const [chatModel, setChatModel] = useState<string | null>(null)
  const [burst, setBurst] = useState(0)
  const [selected, setSelected] = useState(0)
  const listRef = useRef<HTMLDivElement>(null)

  const mode = commandBarInputMode(value, terminalMode)
  const barMode: BarMode = terminalMode ? 'terminal' : mode.isAiMode ? 'ai' : 'search'
  const calc = barMode === 'search' && !mode.isCompletionInput ? evaluateExpression(value) : null

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  // The model chip mirrors the real bar's model picker.
  useEffect(() => {
    if (!aiReady) {
      setChatModel(null)
      return
    }
    void window.tezbar
      .getLlmConfig()
      .then((cfg) => setChatModel(cfg.taskModelOverrides?.chat ?? cfg.model ?? null))
      .catch(() => setChatModel(null))
  }, [aiReady])

  useEffect(() => {
    if (barMode !== 'search') setBurst((count) => count + 1)
  }, [barMode])

  useEffect(() => {
    if (calc) onTaskDone('math')
  }, [calc, onTaskDone])

  // Like the terminal card, getting into the mode is the skill being taught.
  useEffect(() => {
    if (mode.isAiMode) onTaskDone('ai')
  }, [mode.isAiMode, onTaskDone])

  useEffect(() => {
    if (terminalMode) onTaskDone('terminal')
  }, [terminalMode, onTaskDone])

  // The same path completion the real bar calls, against the real disk.
  useEffect(() => {
    if (!mode.isSlashInput || mode.isAiMode) {
      setCompletions(null)
      return
    }
    let cancelled = false
    const query = mode.slashQuery
    const timer = window.setTimeout(() => {
      window.tezbar
        .completePath(query)
        .then((items) => {
          if (cancelled) return
          setCompletions({ query, items })
          setSelected(0)
          if (items.length > 0 && query.length > 1) onTaskDone('folder')
        })
        .catch(() => {
          if (!cancelled) setCompletions({ query, items: [] })
        })
    }, 90)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [mode.isSlashInput, mode.isAiMode, mode.slashQuery, onTaskDone])

  const suggestions = mode.isSlashInput && !mode.isAiMode ? (completions?.items ?? []) : []

  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-index="${selected}"]`)
      ?.scrollIntoView({ block: 'nearest' })
  }, [selected])

  // Same as CommandBar's completePathInput: a folder completes into the
  // bar so you can keep drilling down. Files and apps would open, which
  // practice never does.
  const completeWith = (item: PathCompletionItem): void => {
    if (item.kind === 'directory') setValue(item.value)
    inputRef.current?.focus()
  }

  const tryExample = (id: PracticeTaskId): void => {
    const task = PRACTICE_TASKS.find((item) => item.id === id)
    if (!task) return
    if (id === 'terminal') {
      setValue('')
      setTerminalDir(undefined)
      setTerminalMode(true)
    } else {
      setTerminalMode(false)
      setValue(task.example)
    }
    inputRef.current?.focus()
  }

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>): void => {
    // Mirrors CommandBar: `>` on an empty bar, or right after a /path, swaps
    // into terminal mode rooted at that path.
    if (e.key === '>' && !terminalMode && (!value || mode.isSlashInput) && !mode.isAiMode) {
      e.preventDefault()
      setTerminalDir(mode.isSlashInput ? value.trim() : undefined)
      setValue('')
      setTerminalMode(true)
      return
    }
    if (e.key === 'Backspace' && terminalMode && !value) {
      e.preventDefault()
      setTerminalMode(false)
      if (terminalDir) setValue(terminalDir)
      setTerminalDir(undefined)
      return
    }
    if (e.key === 'Escape') {
      // Clears the practice bar like the real one, and never reaches the
      // launcher's own Esc-to-hide handler.
      e.preventDefault()
      e.stopPropagation()
      setValue('')
      setTerminalMode(false)
      setTerminalDir(undefined)
      return
    }
    if (suggestions.length > 0) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        const step = e.key === 'ArrowDown' ? 1 : -1
        setSelected((index) => (index + step + suggestions.length) % suggestions.length)
        return
      }
      if ((e.key === 'Enter' && !e.metaKey && !e.ctrlKey) || e.key === 'Tab') {
        e.preventDefault()
        const item = suggestions[Math.min(selected, suggestions.length - 1)]
        if (item) completeWith(item)
        return
      }
    }
    if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey) e.preventDefault()
  }

  let response: ReactNode
  if (barMode === 'terminal') {
    response = (
      <>
        <div className="relative flex h-full flex-col justify-between px-4 py-3.5">
          <p className="text-[12.5px] text-emerald-100/90">
            Enter runs{' '}
            {value ? <code className="onboarding-code text-emerald-200">{value}</code> : 'your command'} in a real
            shell, in <code className="onboarding-code text-emerald-200">{terminalDir ? tildify(terminalDir) : '~'}</code>.
          </p>
          <p className="text-[11.5px] text-ink-3">Backspace on an empty line goes back to search.</p>
        </div>
      </>
    )
  } else if (barMode === 'ai') {
    response = (
      <>
        <div className="relative flex h-full flex-col justify-between px-4 py-3.5">
          {mode.aiTask ? (
            <p className="onboarding-cycle-in ml-auto max-w-[80%] rounded-tezbar-row bg-white/[0.08] px-3 py-2 text-[12.5px] text-ink-1">
              {mode.aiTask}
            </p>
          ) : (
            <p className="text-[12.5px] text-ink-2">Ask anything. Pi can read files and run commands, and asks before anything risky.</p>
          )}
          <p className="text-[12.5px] text-violet-200">
            {!aiReady
              ? 'Connect a model in the next step, and Pi answers right here.'
              : mode.aiTask
                ? `Enter, and Pi starts thinking with ${chatModel ?? 'your model'}…`
                : 'Pi is listening.'}
          </p>
        </div>
      </>
    )
  } else if (mode.isSlashInput) {
    response = (
      <div className="flex h-full flex-col">
        <div ref={listRef} role="listbox" aria-label="Folders" className="min-h-0 flex-1 space-y-0.5 overflow-y-auto p-2">
          {suggestions.map((item, index) => (
            <div
              key={item.id}
              role="option"
              aria-selected={index === selected}
              data-index={index}
              onMouseMove={() => setSelected(index)}
              onMouseDown={(e) => {
                e.preventDefault()
                completeWith(item)
              }}
              className={cx(
                'flex h-8 cursor-default items-center gap-2.5 rounded-tezbar-chip px-2.5',
                index === selected && 'bg-accent/15'
              )}
            >
              <Glyph
                d={item.kind === 'directory' ? ICON.folder : item.kind === 'application' ? ICON.app : ICON.file}
                className={index === selected ? 'shrink-0 text-accent-strong' : 'shrink-0 text-ink-4'}
              />
              <span className="truncate text-[12.5px] text-ink-1">{item.title}</span>
              <span className="min-w-0 flex-1 truncate text-right text-[11px] text-ink-4">{tildify(item.subtitle)}</span>
            </div>
          ))}
          {completions && suggestions.length === 0 ? (
            <p className="px-2.5 py-1.5 text-[12px] text-ink-3">No folder matches that path.</p>
          ) : null}
        </div>
        <p className="shrink-0 border-t border-white/[0.06] px-4 py-2 text-[11.5px] leading-snug text-ink-4">
          <Kbd>↑</Kbd> <Kbd>↓</Kbd> select, <Kbd>↵</Kbd> completes. Then <Kbd>{'>'}</Kbd> opens a terminal there.
        </p>
      </div>
    )
  } else if (calc) {
    response = (
      <div className="onboarding-cycle-in px-4 py-3.5">
        <p className="text-[28px] font-semibold tabular-nums text-ink-1">{calc.formatted}</p>
        <p className="mt-1 text-[11.5px] text-ink-4">Enter copies it. Units work too, like 5 km to mi.</p>
      </div>
    )
  } else if (value.startsWith('!')) {
    response = (
      <div className="onboarding-cycle-in px-4 py-3.5">
        <p className="text-[12.5px] text-ink-2">
          DeepSearch looks inside your documents, PDFs, and screenshots for{' '}
          {mode.parsedSearchQuery.query ? (
            <span className="text-ink-1">&ldquo;{mode.parsedSearchQuery.query}&rdquo;</span>
          ) : (
            'your words'
          )}
          .
        </p>
      </div>
    )
  } else {
    response = (
      <div className="onboarding-cycle-in px-4 py-3.5">
        <p className="text-[12.5px] text-ink-3">
          {value.trim()
            ? 'Plain text searches your apps, files, and commands.'
            : 'Type here, or click a card below to fill in an example.'}
        </p>
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <StepHeading title="Try the bar">
        The first character picks the mode: <Kbd>/</Kbd> browses folders, <Kbd>{'>'}</Kbd> opens a terminal, and a
        leading space asks the AI. Nothing here actually runs.
      </StepHeading>

      <div className={cx('onboarding-bar relative mt-5 flex min-h-[176px] flex-1 flex-col', `is-${barMode}`)}>
        <div className="onboarding-bar-surface relative z-10 flex min-h-0 flex-1 flex-col overflow-hidden rounded-tezbar-panel">
          <label className="relative flex h-12 shrink-0 items-center gap-3 px-4">
            {burst > 0 && barMode !== 'search' ? (
              <PuffBurst key={burst} palette={barMode === 'ai' ? AI_PUFF : TERMINAL_PUFF} />
            ) : null}
            <span
              className={cx(
                'relative shrink-0',
                barMode === 'ai' ? 'text-violet-300' : barMode === 'terminal' ? 'text-emerald-300' : 'text-ink-4'
              )}
            >
              <BarIcon mode={barMode} />
            </span>
            {barMode === 'ai' ? (
              <span
                aria-label="AI mode"
                className="relative inline-flex shrink-0 items-center gap-1 rounded-tezbar-chip border border-violet-400/40 bg-violet-500/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-violet-200"
              >
                <span className="h-1.5 w-1.5 rounded-full bg-violet-300" />
                AI
              </span>
            ) : barMode === 'terminal' ? (
              <span className="relative shrink-0 font-mono text-[13px] text-emerald-300/80">
                {terminalDir ? tildify(terminalDir) : '~'} ❯
              </span>
            ) : null}
            <input
              ref={inputRef}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={onKeyDown}
              spellCheck={false}
              autoComplete="off"
              aria-label="Practice command bar"
              placeholder={barMode === 'terminal' ? 'git status' : barMode === 'ai' ? '' : 'Search or type a command…'}
              className={cx(
                'relative min-w-0 flex-1 bg-transparent text-[15px] text-ink-1 outline-none placeholder:text-ink-4',
                barMode === 'terminal' && 'font-mono text-[14px] text-emerald-100 placeholder:text-emerald-200/25'
              )}
            />
            {barMode === 'ai' ? (
              <span className="relative inline-flex shrink-0 items-center gap-2 rounded-tezbar-row border border-white/10 bg-white/[0.03] px-2.5 py-1 font-mono text-[11.5px] text-ink-2">
                <span className={cx('h-1.5 w-1.5 rounded-full', chatModel ? 'bg-emerald-400' : 'bg-ink-4')} />
                {chatModel ?? 'no model yet'}
              </span>
            ) : null}
          </label>
          <div className="relative min-h-0 flex-1 border-t border-white/[0.07]">{response}</div>
        </div>
      </div>

      <div className="mt-3 grid shrink-0 grid-cols-2 gap-2">
        {PRACTICE_TASKS.map((task) => {
          const isDone = done.has(task.id)
          return (
            <button
              key={task.id}
              type="button"
              onClick={() => tryExample(task.id)}
              className={cx(
                'flex items-center gap-3 rounded-tezbar-row border px-3 py-2.5 text-left transition-colors',
                isDone
                  ? 'border-emerald-400/20 bg-emerald-400/[0.05]'
                  : 'border-white/[0.07] bg-white/[0.02] hover:border-white/15 hover:bg-white/[0.04]'
              )}
            >
              <span
                className={cx(
                  'flex h-7 min-w-[28px] shrink-0 items-center justify-center rounded-[7px] px-1.5 font-mono text-[11.5px]',
                  isDone ? 'bg-emerald-400/15 text-emerald-300' : 'bg-white/[0.06] text-ink-2'
                )}
              >
                {isDone ? <Glyph d={ICON.check} className="onboarding-chip-pop" /> : task.trigger}
              </span>
              <span className="min-w-0">
                <span className="block text-[12.5px] font-medium text-ink-1">{task.title}</span>
                <span className="block truncate text-[11px] text-ink-4">{task.hint}</span>
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )
}

/* =========================================================================
   Step 3 — Pi + a real, verified provider connection
   ========================================================================= */
type QuickProvider = 'anthropic' | 'openai' | 'ollama'

const QUICK_PROVIDERS: { id: QuickProvider; title: string; note: string; keyUrl?: string; keyPlaceholder?: string }[] = [
  {
    id: 'anthropic',
    title: 'Anthropic',
    note: 'Claude models',
    keyUrl: 'https://console.anthropic.com/settings/keys',
    keyPlaceholder: 'sk-ant-…',
  },
  {
    id: 'openai',
    title: 'OpenAI',
    note: 'GPT models',
    keyUrl: 'https://platform.openai.com/api-keys',
    keyPlaceholder: 'sk-…',
  },
  { id: 'ollama', title: 'Ollama', note: `Runs on ${MACHINE}` },
]

/** Providers that count as "configured" out of the box (they ship default
 *  model lists), so a fresh install would falsely read as connected. */
const UNVERIFIED_BY_DEFAULT = new Set<ProviderId>(['ollama', 'opencode', 'antigravity'])

function pickModel(provider: QuickProvider, models: string[]): string {
  const recommended = recommendedModel(provider)
  if (models.includes(recommended)) return recommended
  if (provider === 'openai') return models.find((id) => /^gpt-/.test(id)) ?? recommended
  return models[0] ?? recommended
}

function buildProviderPatch(
  config: LlmConfigRecord,
  provider: QuickProvider,
  apiKey: string,
  models: string[]
): LlmConfigRecord {
  const model = pickModel(provider, models)
  const baseURL = defaultBaseUrl(provider)
  const patch: LlmConfigRecord = {
    provider,
    model,
    baseURL,
    providerConfigs: {
      ...config.providerConfigs,
      [provider]: { ...config.providerConfigs?.[provider], baseURL, ...(apiKey ? { apiKey } : {}) },
    },
    providerSelectedModels: { ...config.providerSelectedModels, [provider]: model },
    taskProviderOverrides: { ...config.taskProviderOverrides, chat: provider },
    taskModelOverrides: { ...config.taskModelOverrides, chat: model },
  }
  if (apiKey) patch.apiKey = apiKey
  if (provider === 'ollama') {
    patch.providerModels = {
      ...config.providerModels,
      ollama: normalizeProviderModelList(
        'ollama',
        models.map((id) => ({ id, capabilities: inferCapabilities(id) }))
      ),
    }
  }
  return patch
}

function PiRow(): JSX.Element {
  const [status, setStatus] = useState<'checking' | 'ready' | 'missing' | 'installing'>('checking')
  const [version, setVersion] = useState<string | undefined>()
  const [failure, setFailure] = useState<{ text: string; url?: string } | null>(null)

  useEffect(() => {
    let cancelled = false
    window.tezbar
      .checkPiAgent()
      .then((result) => {
        if (cancelled) return
        setStatus(result.installed ? 'ready' : 'missing')
        setVersion(result.version)
      })
      .catch(() => {
        if (!cancelled) setStatus('missing')
      })
    return () => {
      cancelled = true
    }
  }, [])

  const install = async (): Promise<void> => {
    setStatus('installing')
    setFailure(null)
    try {
      const result = await window.tezbar.installPiAgent()
      if (result.ok) {
        setVersion(result.version)
        setStatus('ready')
        return
      }
      setStatus('missing')
      setFailure({
        text:
          result.reason === 'no-package-manager'
            ? 'Installing Pi needs Node.js (npm), which isn’t on this machine.'
            : result.error,
        url: result.officialUrl,
      })
    } catch (error) {
      setStatus('missing')
      setFailure({ text: error instanceof Error ? error.message : 'The install didn’t finish.' })
    }
  }

  return (
    <div className="rounded-tezbar-row border border-white/[0.07] bg-white/[0.02] px-4 py-3">
      <div className="flex items-center gap-3">
        <span
          className={cx(
            'h-2 w-2 shrink-0 rounded-full',
            status === 'ready' ? 'bg-emerald-400' : status === 'missing' ? 'bg-amber-300' : 'bg-ink-4 animate-pulse'
          )}
          aria-hidden
        />
        <div className="min-w-0 flex-1">
          <p className="text-[12.5px] font-medium text-ink-1">Pi agent</p>
          <p className="text-[11.5px] text-ink-3">
            {status === 'checking'
              ? `Looking for Pi on ${MACHINE}…`
              : status === 'installing'
                ? 'Installing @mariozechner/pi-coding-agent… this can take a minute.'
                : status === 'ready'
                  ? `Installed${version ? `, version ${version}` : ''}. Tezbar will use it for AI chat.`
                  : 'Not installed. Tezbar can install it with npm.'}
          </p>
        </div>
        {status === 'missing' || status === 'installing' ? (
          <Button variant="ghost" onClick={() => void install()} disabled={status === 'installing'}>
            {status === 'installing' ? 'Installing…' : 'Install Pi'}
          </Button>
        ) : null}
      </div>
      {failure ? (
        <div className="mt-2 flex items-start justify-between gap-3 pl-5">
          <p className="line-clamp-2 text-[11.5px] leading-snug text-rose-300">{failure.text}</p>
          {failure.url ? (
            <button
              type="button"
              className="shrink-0 text-[11.5px] text-accent-strong underline-offset-2 hover:underline"
              onClick={() => void window.tezbar.openExternalUrl(failure.url as string)}
            >
              Install instructions
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

function AiStep({ onSummary }: { onSummary: (summary: StepSummary) => void }): JSX.Element {
  const [config, setConfig] = useState<LlmConfigRecord | null>(null)
  const [connected, setConnected] = useState<{ provider: ProviderId; model: string } | null>(null)
  const [choice, setChoice] = useState<QuickProvider>('anthropic')
  const [apiKey, setApiKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [editing, setEditing] = useState(false)

  useEffect(() => {
    void window.tezbar
      .getLlmConfig()
      .then((cfg) => {
        setConfig(cfg)
        const provider = cfg.provider
        if (provider && !UNVERIFIED_BY_DEFAULT.has(provider) && isAiProviderConfigured(cfg, provider)) {
          setConnected({ provider, model: cfg.taskModelOverrides?.chat ?? cfg.model ?? recommendedModel(provider) })
        }
      })
      .catch(() => setConfig({}))
  }, [])

  useEffect(() => {
    onSummary(
      connected
        ? { done: true, detail: providerTitle(connected.provider, config ?? undefined) }
        : { done: false, detail: 'Not connected' }
    )
  }, [connected, config, onSummary])

  const provider = (QUICK_PROVIDERS.find((item) => item.id === choice) ?? QUICK_PROVIDERS[0]) as (typeof QUICK_PROVIDERS)[number]
  const needsKey = choice !== 'ollama'

  const connect = async (): Promise<void> => {
    if (needsKey && !apiKey.trim()) {
      setError(`Paste your ${provider.title} API key first.`)
      return
    }
    setBusy(true)
    setError(null)
    try {
      const key = apiKey.trim()
      // Listing models is a cheap authenticated call — if it returns nothing,
      // the key was rejected or the server isn't reachable.
      const models = await window.tezbar.listLlmModels(choice, undefined, needsKey ? key : undefined)
      if (models.length === 0) {
        setError(
          choice === 'ollama'
            ? 'Ollama isn’t answering at localhost:11434, or has no models pulled. Start Ollama, run “ollama pull llama3.2”, then try again.'
            : `${provider.title} didn’t accept that key. Check it was copied in full, then try again.`
        )
        return
      }
      const latest = await window.tezbar.getLlmConfig().catch(() => config ?? {})
      const patch = buildProviderPatch(latest, choice, needsKey ? key : '', models)
      await window.tezbar.setLlmConfig(patch)
      setConfig({ ...latest, ...patch })
      setConnected({ provider: choice, model: patch.model ?? recommendedModel(choice) })
      setApiKey('')
      setEditing(false)
    } catch {
      setError('Couldn’t save the connection. You can finish this later in Settings → AI.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <StepHeading title="Connect an AI model">
        AI chat runs on Pi, an open-source coding agent that can read files and run commands for you, asking
        before anything risky. Bring your own key, or use a local model.
      </StepHeading>

      <div className="mt-5 space-y-3">
        <PiRow />

        {connected && !editing ? (
          <div className="onboarding-cycle-in flex items-center gap-3 rounded-tezbar-row border border-emerald-400/20 bg-emerald-400/[0.05] px-4 py-3">
            <Glyph d={ICON.check} className="shrink-0 text-emerald-300" />
            <p className="min-w-0 flex-1 text-[12.5px] text-ink-2">
              Connected to <span className="text-ink-1">{providerTitle(connected.provider, config ?? undefined)}</span>,
              using <span className="font-mono text-[11.5px] text-ink-1">{connected.model}</span>
            </p>
            <Button variant="quiet" onClick={() => setEditing(true)}>
              Change
            </Button>
          </div>
        ) : (
          <div className="rounded-tezbar-row border border-white/[0.07] bg-white/[0.02] p-3">
            <div className="grid grid-cols-3 gap-2" role="radiogroup" aria-label="AI provider">
              {QUICK_PROVIDERS.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  role="radio"
                  aria-checked={choice === item.id}
                  onClick={() => {
                    setChoice(item.id)
                    setError(null)
                  }}
                  className={cx(
                    'rounded-tezbar-row border px-3 py-2 text-left transition-colors',
                    choice === item.id
                      ? 'border-accent/45 bg-accent/10'
                      : 'border-white/[0.07] hover:border-white/15 hover:bg-white/[0.03]'
                  )}
                >
                  <span className="block text-[12.5px] font-medium text-ink-1">{item.title}</span>
                  <span className="block text-[11px] text-ink-4">{item.note}</span>
                </button>
              ))}
            </div>

            <form
              className="mt-3 flex items-center gap-2"
              onSubmit={(e) => {
                e.preventDefault()
                void connect()
              }}
            >
              {needsKey ? (
                <input
                  key={choice}
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder={`${provider.title} API key (${provider.keyPlaceholder})`}
                  aria-label={`${provider.title} API key`}
                  autoComplete="off"
                  spellCheck={false}
                  className="h-9 min-w-0 flex-1 rounded-tezbar-chip border border-white/10 bg-black/25 px-3 font-mono text-[12px] text-ink-1 outline-none placeholder:font-sans placeholder:text-ink-4 focus:border-accent/50"
                />
              ) : (
                <p className="min-w-0 flex-1 text-[12px] leading-snug text-ink-3">
                  Uses the models Ollama is serving at localhost:11434. Nothing leaves {MACHINE}.
                </p>
              )}
              <Button type="submit" variant="primary" disabled={busy} className="min-w-[92px]">
                {busy ? 'Checking…' : 'Connect'}
              </Button>
            </form>
            <div className="mt-2 flex min-h-[18px] items-start justify-between gap-3">
              {error ? (
                <Message tone="error">{error}</Message>
              ) : needsKey ? (
                <p className="text-[11.5px] text-ink-4">Your key is stored on {MACHINE} and sent only to {provider.title}.</p>
              ) : (
                <span />
              )}
              {provider.keyUrl ? (
                <button
                  type="button"
                  className="shrink-0 text-[11.5px] text-accent-strong underline-offset-2 hover:underline"
                  onClick={() => void window.tezbar.openExternalUrl(provider.keyUrl as string)}
                >
                  Get a key
                </button>
              ) : null}
            </div>
          </div>
        )}

        <p className="text-[11.5px] text-ink-4">
          Gemini, DeepSeek, GitHub Copilot, and OpenAI-compatible servers are in Settings → AI.
        </p>
      </div>
    </>
  )
}

/* =========================================================================
   Step 4 — DeepSearch folders, added and indexed for real
   ========================================================================= */
function indexingDetail(status: KnowledgeStatus | undefined, rootCount: number): string {
  if (rootCount === 0) return 'No folders yet'
  if (!status) return `${rootCount} ${rootCount === 1 ? 'folder' : 'folders'}`
  if (status.state === 'failed') return 'Indexing stopped'
  if (status.state === 'completed' || status.state === 'idle') {
    return `${status.sourceCount.toLocaleString()} files searchable`
  }
  if (status.state === 'paused') return `Paused at ${Math.round(status.progress * 100)}%`
  return `Indexing ${Math.round(status.progress * 100)}%`
}

function DeepSearchStep({ onSummary }: { onSummary: (summary: StepSummary) => void }): JSX.Element {
  const [snapshot, setSnapshot] = useState<KnowledgeSnapshot | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    void window.tezbar
      .getKnowledgeSnapshot()
      .then(setSnapshot)
      .catch(() => setError('DeepSearch isn’t available right now. You can set it up later in Settings → Knowledge.'))
    return window.tezbar.onKnowledgeStatus((status) => {
      setSnapshot((current) => (current ? { ...current, status } : current))
    })
  }, [])

  const roots = snapshot?.roots.filter((root) => root.enabled) ?? []
  const status = snapshot?.status

  useEffect(() => {
    onSummary({ done: roots.length > 0, detail: indexingDetail(status, roots.length) })
  }, [roots.length, status, onSummary])

  const run = async (action: () => Promise<KnowledgeSnapshot | null>): Promise<void> => {
    setBusy(true)
    setError(null)
    try {
      const next = await action()
      if (next) setSnapshot(next)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'That folder couldn’t be added.')
    } finally {
      setBusy(false)
    }
  }

  const addFolder = (): Promise<void> =>
    run(async () => {
      const path = await window.tezbar.chooseKnowledgeFolder()
      return path ? window.tezbar.addKnowledgeRoot(path) : null
    })

  const removeRoot = (rootId: string): Promise<void> => run(() => window.tezbar.removeKnowledgeRoot(rootId))

  // Mirrors MAJOR_KNOWLEDGE_FOLDER_NAMES in the knowledge service.
  const majorNames = ['Desktop', 'Documents', 'Downloads', 'Pictures']
  const rootNames = new Set(roots.map((root) => root.path.split(/[\\/]/).filter(Boolean).pop()))
  const hasMajorRoots = majorNames.every((name) => rootNames.has(name))
  const running = status?.state === 'indexing' || status?.state === 'scanning'
  const paused = status?.state === 'paused'
  const percent = Math.round((status?.progress ?? 0) * 100)
  // Same numbers Settings shows: processed so far, plus what's still queued.
  const total = (status?.processedSources ?? 0) + (status?.queuedSources ?? 0)

  const pause = (): Promise<void> => {
    setSnapshot((current) => (current ? { ...current, status: { ...current.status, state: 'paused' } } : current))
    return run(() => window.tezbar.pauseKnowledgeIndexing())
  }
  const resume = (): Promise<void> => run(() => window.tezbar.resumeKnowledgeIndexing())

  return (
    <>
      <StepHeading title="Choose what DeepSearch reads">
        DeepSearch reads the text inside documents, PDFs, and screenshots, with OCR running on {MACHINE}, so you
        can find a file by what it says. Start a search with <Kbd>!</Kbd> to use it.
      </StepHeading>

      <div className="mt-5 rounded-tezbar-row border border-white/[0.07] bg-white/[0.02]">
        <div className="max-h-[204px] min-h-[120px] overflow-y-auto p-1.5">
          {!snapshot && !error ? (
            <div className="h-[48px] animate-pulse rounded-tezbar-chip bg-white/[0.03]" />
          ) : roots.length === 0 ? (
            <div className="flex h-[108px] flex-col items-center justify-center gap-1 text-center">
              <p className="text-[12.5px] text-ink-2">No folders yet</p>
              <p className="text-[11.5px] text-ink-4">Add the folders where your documents live.</p>
            </div>
          ) : (
            roots.map((root) => (
              <div key={root.id} className="group flex items-center gap-3 rounded-tezbar-chip px-2.5 py-2 hover:bg-white/[0.03]">
                <Glyph d={ICON.folder} className="shrink-0 text-accent-strong" />
                <span className="text-[12.5px] text-ink-1">{root.path.split(/[\\/]/).filter(Boolean).pop()}</span>
                <span className="min-w-0 flex-1 truncate text-[11px] text-ink-4">{tildify(root.path)}</span>
                <button
                  type="button"
                  aria-label={`Stop indexing ${root.path}`}
                  onClick={() => void removeRoot(root.id)}
                  disabled={busy}
                  className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-ink-4 opacity-0 transition hover:bg-white/[0.08] hover:text-ink-2 focus-visible:opacity-100 group-hover:opacity-100"
                >
                  <Glyph d={ICON.close} size={12} />
                </button>
              </div>
            ))
          )}
        </div>
        <div className="flex items-center gap-2 border-t border-white/[0.07] px-3 py-2.5">
          {!hasMajorRoots ? (
            <Button
              variant={roots.length === 0 ? 'primary' : 'ghost'}
              onClick={() => void run(() => window.tezbar.addMajorKnowledgeRoots())}
              disabled={busy || !snapshot}
            >
              Add Desktop, Documents, Downloads & Pictures
            </Button>
          ) : null}
          <Button variant="ghost" onClick={() => void addFolder()} disabled={busy || !snapshot}>
            Add folder…
          </Button>
        </div>
      </div>

      <div className="mt-3 min-h-[34px]">
        {error ? (
          <Message tone="error">{error}</Message>
        ) : roots.length > 0 && status ? (
          <div className="space-y-2">
            <div className="flex items-center gap-3 text-[11.5px]">
              <span className="min-w-0 flex-1 text-ink-3">
                {status.state === 'scanning'
                  ? 'Finding files to index…'
                  : running
                    ? `Indexing ${status.processedSources.toLocaleString()} of ${total.toLocaleString()} files`
                    : paused
                      ? `Paused after ${status.processedSources.toLocaleString()} of ${total.toLocaleString()} files. Nothing is read until you resume.`
                      : status.state === 'failed'
                        ? status.error ?? 'Indexing stopped.'
                        : `${status.sourceCount.toLocaleString()} files indexed. New files are picked up automatically.`}
              </span>
              {running || paused ? <span className="tabular-nums text-ink-4">{percent}%</span> : null}
              {running ? (
                <Button variant="ghost" onClick={() => void pause()} disabled={busy} className="shrink-0">
                  Pause
                </Button>
              ) : paused ? (
                <Button variant="primary" onClick={() => void resume()} disabled={busy} className="shrink-0">
                  Resume
                </Button>
              ) : null}
            </div>
            {running || paused ? (
              <div className="h-1 overflow-hidden rounded-full bg-white/[0.06]">
                <div
                  className={cx(
                    'h-full rounded-full transition-[width] duration-500',
                    paused ? 'bg-amber-300/70' : 'bg-accent-strong'
                  )}
                  style={{ width: `${Math.max(2, percent)}%` }}
                />
              </div>
            ) : null}
            {running ? (
              <p className="text-[11px] text-ink-4">Keeps going in the background after setup. Pause it anytime here or in Settings → Knowledge.</p>
            ) : null}
          </div>
        ) : null}
      </div>
    </>
  )
}

/* =========================================================================
   Step 5 — permissions, re-probed while the step is open
   ========================================================================= */
const STATE_TEXT: Record<PermissionState, string> = {
  granted: 'Allowed',
  denied: 'Denied. Turn it on in System Settings.',
  restricted: 'Blocked by a device policy',
  'not-determined': 'Not allowed yet',
  unsupported: 'Not needed here',
}

function PermissionsStep({ onSummary }: { onSummary: (summary: StepSummary) => void }): JSX.Element {
  const [snapshot, setSnapshot] = useState<PermissionsSnapshot | null>(null)
  const [pending, setPending] = useState<PermissionId | null>(null)

  const reload = useCallback(async () => {
    try {
      setSnapshot(await window.tezbar.getPermissions())
    } catch {
      // Keep the last snapshot; the step still works as an informational list.
    }
  }, [])

  // Most grants happen in System Settings, outside this window, so poll
  // while the step is open to reflect them without a manual refresh.
  useEffect(() => {
    void reload()
    const timer = window.setInterval(() => void reload(), 2500)
    return () => window.clearInterval(timer)
  }, [reload])

  const featured = useMemo(() => {
    if (!snapshot) return []
    return FEATURED_PERMISSION_IDS.map((id) => snapshot.statuses.find((status) => status.descriptor.id === id)).filter(
      (status): status is NonNullable<typeof status> => Boolean(status)
    )
  }, [snapshot])

  useEffect(() => {
    if (featured.length === 0) return
    const granted = featured.filter((status) => status.state === 'granted' || status.state === 'unsupported').length
    onSummary({ done: granted === featured.length, detail: `${granted} of ${featured.length} allowed` })
  }, [featured, onSummary])

  const grant = async (id: PermissionId, settingsUrl?: string, denied?: boolean): Promise<void> => {
    setPending(id)
    try {
      // macOS never re-prompts after a denial; only the System Settings
      // toggle can flip it back, so go straight there.
      if (denied && settingsUrl) await window.tezbar.openExternalUrl(settingsUrl)
      else await window.tezbar.requestPermission(id)
    } finally {
      await reload()
      setPending(null)
    }
  }

  return (
    <>
      <StepHeading title="Allow what you’ll use">
        Each of these is optional. macOS asks you to confirm, and you can change them anytime in Settings →
        Permissions.
      </StepHeading>

      <ul className="mt-5 space-y-2">
        {!snapshot
          ? [0, 1, 2].map((index) => (
              <li key={index} className="h-[66px] animate-pulse rounded-tezbar-row border border-white/[0.07] bg-white/[0.02]" />
            ))
          : featured.map((status) => {
              const granted = status.state === 'granted' || status.state === 'unsupported'
              const denied = status.state === 'denied' && Boolean(status.descriptor.settingsUrl)
              return (
                <li
                  key={status.descriptor.id}
                  className="flex items-center gap-4 rounded-tezbar-row border border-white/[0.07] bg-white/[0.02] px-4 py-3"
                >
                  <span
                    className={cx(
                      'flex h-6 w-6 shrink-0 items-center justify-center rounded-full',
                      granted ? 'bg-emerald-400/15 text-emerald-300' : 'bg-white/[0.05] text-ink-4'
                    )}
                  >
                    {granted ? <Glyph d={ICON.check} size={13} className="onboarding-chip-pop" /> : <span className="h-1.5 w-1.5 rounded-full bg-current" />}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="text-[12.5px] font-medium text-ink-1">{status.descriptor.title}</p>
                    <p className="mt-0.5 text-[11.5px] leading-snug text-ink-3">{status.descriptor.rationale}</p>
                    <p className={cx('mt-1 text-[11px]', granted ? 'text-emerald-300' : status.state === 'denied' ? 'text-rose-300' : 'text-ink-4')}>
                      {STATE_TEXT[status.state]}
                    </p>
                  </div>
                  {!granted ? (
                    <Button
                      variant="ghost"
                      onClick={() => void grant(status.descriptor.id, status.descriptor.settingsUrl, denied)}
                      disabled={pending === status.descriptor.id}
                      className="min-w-[84px] shrink-0"
                    >
                      {pending === status.descriptor.id ? 'Opening…' : denied ? 'Open Settings' : 'Allow'}
                    </Button>
                  ) : null}
                </li>
              )
            })}
      </ul>
      <p className="mt-3 text-[11.5px] text-ink-4">
        Automation, Input Monitoring, and Calendar can be allowed anytime in Settings → Permissions.
      </p>
    </>
  )
}

/* =========================================================================
   Shared layout
   ========================================================================= */
function StepHeading({ title, children }: { title: string; children: ReactNode }): JSX.Element {
  return (
    <header>
      <h2 className="text-[20px] font-semibold leading-tight text-ink-1">{title}</h2>
      <p className="mt-2 max-w-[460px] text-[12.5px] leading-relaxed text-ink-3">{children}</p>
    </header>
  )
}

function SplashScreen({ exiting }: { exiting: boolean }): JSX.Element {
  return (
    <div
      className={cx(
        'flex h-full w-full flex-col items-center justify-center gap-5 text-center',
        exiting && 'onboarding-splash-exit'
      )}
    >
      <div className="relative h-20 w-20">
        <span className="onboarding-orb absolute inset-0 rounded-full bg-accent/30 blur-2xl" aria-hidden />
        <div className="relative z-10 h-20 w-20 overflow-hidden rounded-tezbar-panel border border-white/10 bg-white/[0.04] shadow-[0_0_28px_rgba(139,141,247,0.35)]">
          <img src={appIconUrl} alt="" className="h-full w-full object-cover" />
        </div>
      </div>
      <div className="space-y-2">
        <h1 className="text-[28px] font-semibold text-ink-1">Welcome to Tezbar</h1>
        <p className="text-[13px] text-ink-3">Five quick steps and it’s set up the way you work.</p>
      </div>
      <p className="onboarding-splash-hint text-[11px] text-ink-4">Press any key to start</p>
    </div>
  )
}

function StepRail({
  current,
  summaries,
  onSelect,
  onSkip,
}: {
  current: number
  summaries: Record<StepId, StepSummary>
  onSelect: (index: number) => void
  onSkip: () => void
}): JSX.Element {
  return (
    <nav aria-label="Setup steps" className="flex w-[200px] shrink-0 flex-col border-r border-white/[0.06] px-3 py-5">
      <div className="flex items-center gap-2.5 px-2">
        <img src={appIconUrl} alt="" className="h-7 w-7 rounded-[8px]" />
        <span className="text-[13px] font-semibold text-ink-1">Set up Tezbar</span>
      </div>

      <ol className="mt-6 flex-1">
        {STEPS.map((step, index) => {
          const summary = summaries[step.id]
          const isCurrent = index === current
          const isLast = index === STEPS.length - 1
          return (
            <li key={step.id} className="relative">
              {!isLast ? (
                <span
                  className={cx(
                    'absolute left-[19px] top-[30px] h-[calc(100%-22px)] w-px',
                    summary.done ? 'bg-emerald-400/30' : 'bg-white/[0.08]'
                  )}
                  aria-hidden
                />
              ) : null}
              <button
                type="button"
                onClick={() => onSelect(index)}
                aria-current={isCurrent ? 'step' : undefined}
                className={cx(
                  'flex w-full items-start gap-3 rounded-tezbar-row px-2 py-2 text-left transition-colors',
                  isCurrent ? 'bg-white/[0.05]' : 'hover:bg-white/[0.03]'
                )}
              >
                <span
                  className={cx(
                    'relative z-10 mt-[1px] flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full border',
                    summary.done
                      ? 'border-emerald-400/40 bg-emerald-400/15 text-emerald-300'
                      : isCurrent
                        ? 'border-accent-strong bg-accent/20'
                        : 'border-white/15'
                  )}
                  aria-hidden
                >
                  {summary.done ? (
                    <Glyph d={ICON.check} size={12} className="onboarding-chip-pop" />
                  ) : isCurrent ? (
                    <span className="h-1.5 w-1.5 rounded-full bg-accent-strong" />
                  ) : null}
                </span>
                <span className="min-w-0">
                  <span className={cx('block text-[12.5px]', isCurrent ? 'font-medium text-ink-1' : 'text-ink-2')}>
                    {step.label}
                  </span>
                  <span className="block truncate text-[11px] text-ink-4">{summary.detail}</span>
                </span>
              </button>
            </li>
          )
        })}
      </ol>

      <button
        type="button"
        onClick={onSkip}
        className="mx-2 self-start text-[11.5px] text-ink-4 underline-offset-2 transition hover:text-ink-2 hover:underline"
      >
        Skip setup
      </button>
    </nav>
  )
}

/* =========================================================================
   Onboarding
   ========================================================================= */
export default function OnboardingView({ onDone }: { onDone: () => void }): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const [splashDone, setSplashDone] = useState(false)
  const [splashExiting, setSplashExiting] = useState(false)
  const [stepIndex, setStepIndex] = useState(0)
  const [direction, setDirection] = useState<'forward' | 'back'>('forward')
  const [hotkey, setHotkey] = useState(DEFAULT_HOTKEY)
  const [practiceDone, setPracticeDone] = useState<ReadonlySet<PracticeTaskId>>(() => new Set())
  const [summaries, setSummaries] = useState<Record<StepId, StepSummary>>({
    shortcut: { done: true, detail: shortcutLabel(DEFAULT_HOTKEY) },
    practice: { done: false, detail: `0 of ${PRACTICE_TASKS.length} tried` },
    ai: { done: false, detail: 'Not connected' },
    deepsearch: { done: false, detail: 'No folders yet' },
    permissions: { done: false, detail: 'Optional' },
  })
  const finishingRef = useRef(false)
  const splashDismissingRef = useRef(false)

  const report = useCallback((id: StepId, summary: StepSummary) => {
    setSummaries((current) =>
      current[id].done === summary.done && current[id].detail === summary.detail ? current : { ...current, [id]: summary }
    )
  }, [])
  const reportAi = useCallback((summary: StepSummary) => report('ai', summary), [report])
  const reportDeepSearch = useCallback((summary: StepSummary) => report('deepsearch', summary), [report])
  const reportPermissions = useCallback((summary: StepSummary) => report('permissions', summary), [report])

  useEffect(() => {
    rootRef.current?.focus()
    void window.tezbar
      .getLlmConfig()
      .then((config) => {
        if (typeof config.raymesHotkey === 'string' && config.raymesHotkey.length > 0) {
          setHotkey(config.raymesHotkey)
        }
        const provider = config.provider
        if (provider && !UNVERIFIED_BY_DEFAULT.has(provider) && isAiProviderConfigured(config, provider)) {
          report('ai', { done: true, detail: providerTitle(provider, config) })
        }
      })
      .catch(() => undefined)
    void window.tezbar
      .getKnowledgeSnapshot()
      .then((snap) => {
        const roots = snap.roots.filter((root) => root.enabled).length
        report('deepsearch', { done: roots > 0, detail: indexingDetail(snap.status, roots) })
      })
      .catch(() => undefined)
  }, [report])

  useEffect(() => {
    report('shortcut', { done: true, detail: shortcutLabel(hotkey) })
  }, [hotkey, report])

  const onPracticeTaskDone = useCallback((id: PracticeTaskId) => {
    setPracticeDone((current) => (current.has(id) ? current : new Set(current).add(id)))
  }, [])

  useEffect(() => {
    report('practice', {
      done: practiceDone.size === PRACTICE_TASKS.length,
      detail: `${practiceDone.size} of ${PRACTICE_TASKS.length} tried`,
    })
  }, [practiceDone, report])

  const dismissSplash = useCallback(() => {
    if (splashDismissingRef.current) return
    splashDismissingRef.current = true
    setSplashExiting(true)
    window.setTimeout(() => setSplashDone(true), SPLASH_EXIT_MS)
  }, [])

  useEffect(() => {
    const timer = window.setTimeout(dismissSplash, SPLASH_AUTO_DISMISS_MS)
    return () => window.clearTimeout(timer)
  }, [dismissSplash])

  const finish = useCallback(async (): Promise<void> => {
    if (finishingRef.current) return
    finishingRef.current = true
    try {
      await window.tezbar.setLlmConfig({ hasCompletedOnboarding: true })
    } catch {
      // Even if the write fails, don't trap the user on the onboarding surface.
    } finally {
      onDone()
    }
  }, [onDone])

  const goTo = useCallback((index: number) => {
    setStepIndex((current) => {
      setDirection(index >= current ? 'forward' : 'back')
      return Math.max(0, Math.min(LAST_STEP, index))
    })
  }, [])

  const goNext = useCallback(() => {
    if (stepIndex >= LAST_STEP) {
      void finish()
      return
    }
    goTo(stepIndex + 1)
  }, [stepIndex, finish, goTo])

  const goBack = useCallback(() => goTo(stepIndex - 1), [stepIndex, goTo])

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (!splashDone) {
        event.preventDefault()
        event.stopPropagation()
        dismissSplash()
        return
      }
      // Esc must never throw away the whole setup by accident; "Skip setup"
      // is the explicit way out. Swallow it so the launcher doesn't hide.
      if (event.key === 'Escape') {
        if (isTypingTarget(event.target)) return
        event.preventDefault()
        event.stopPropagation()
        return
      }
      if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        event.stopPropagation()
        goNext()
        return
      }
      if (isTypingTarget(event.target) || event.target instanceof HTMLButtonElement) return
      if (event.key === 'Enter' || event.key === 'ArrowRight') {
        event.preventDefault()
        goNext()
      } else if (event.key === 'ArrowLeft') {
        event.preventDefault()
        goBack()
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [splashDone, dismissSplash, goNext, goBack])

  if (!splashDone) {
    return (
      <div
        ref={rootRef}
        tabIndex={-1}
        role="application"
        aria-label="Onboarding"
        className="flex h-full min-h-0 w-full flex-col outline-none"
        onClick={dismissSplash}
      >
        <SplashScreen exiting={splashExiting} />
      </div>
    )
  }

  const step = STEPS[stepIndex] as (typeof STEPS)[number]
  const isLast = stepIndex === LAST_STEP

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Onboarding"
      className="glass-card flex h-full min-h-0 w-full overflow-hidden p-0 outline-none"
    >
      <StepRail current={stepIndex} summaries={summaries} onSelect={goTo} onSkip={() => void finish()} />

      <section className="flex min-w-0 flex-1 flex-col">
        <div
          key={step.id}
          className={cx(
            'min-h-0 flex-1 overflow-y-auto px-7 pb-4 pt-7',
            direction === 'back' ? 'onboarding-step-back' : 'onboarding-step-forward'
          )}
        >
          {step.id === 'shortcut' ? (
            <ShortcutStep hotkey={hotkey} onHotkeyChange={setHotkey} />
          ) : step.id === 'practice' ? (
            <PracticeStep done={practiceDone} onTaskDone={onPracticeTaskDone} aiReady={summaries.ai.done} />
          ) : step.id === 'ai' ? (
            <AiStep onSummary={reportAi} />
          ) : step.id === 'deepsearch' ? (
            <DeepSearchStep onSummary={reportDeepSearch} />
          ) : (
            <PermissionsStep onSummary={reportPermissions} />
          )}
        </div>

        <footer className="flex shrink-0 items-center justify-between border-t border-white/[0.06] px-7 py-3">
          <span className="flex items-center gap-1.5 text-[11px] text-ink-4">
            <Kbd>{IS_WINDOWS ? 'Ctrl' : '⌘'}</Kbd>
            <Kbd>↵</Kbd>
            <span className="ml-0.5">{isLast ? 'Finish' : 'Continue'}</span>
          </span>
          <div className="flex items-center gap-2">
            {stepIndex > 0 ? (
              <Button variant="ghost" onClick={goBack}>
                Back
              </Button>
            ) : null}
            <Button variant="primary" onClick={goNext} className="min-w-[112px]">
              {isLast ? 'Start using Tezbar' : summaries[step.id].done || step.id === 'shortcut' ? 'Continue' : 'Skip for now'}
            </Button>
          </div>
        </footer>
      </section>
    </div>
  )
}
