import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Terminal, type ITerminalOptions } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { cx, Hint, HintBar, Kbd, SelectField, TextField } from './ui/primitives'
import { readTerminalDefaults, type TerminalDefaults } from './terminalPreferences'
import {
  terminalSessionRestoreOptions,
  terminalSessionShortcutIndex,
} from './terminalSessionSelection'
import { terminalOutputAfterSnapshot } from './terminalSessionHistory'
import {
  commandFromTerminalLine,
  formatTerminalSessionName,
  normalizeTerminalCommand,
  terminalDirectoryLabel,
} from './terminalSessionLabel'
import {
  type TerminalDataEvent,
  type TerminalExitEvent,
  type TerminalKeepAliveFor,
  type TerminalSaveFor,
  type TerminalSessionSummary,
} from '../shared/terminal'

const SAVE_OPTIONS: Array<{ value: TerminalSaveFor; label: string }> = [
  { value: 'day', label: 'Day' },
  { value: 'week', label: 'Week' },
  { value: 'month', label: 'Month' },
  { value: 'forever', label: 'Forever' },
]

const KEEP_ALIVE_OPTIONS: Array<{ value: TerminalKeepAliveFor; label: string }> = [
  { value: '3h', label: '3 hours' },
  { value: '8h', label: '8 hours' },
  { value: 'day', label: 'Day' },
  { value: 'until-stop', label: 'Until stop' },
]

const DEFAULT_TERMINAL_FONT_SIZE = 12
const MIN_TERMINAL_FONT_SIZE = 8
const MAX_TERMINAL_FONT_SIZE = 24
const TERMINAL_ZOOM_STEP = 1

type TerminalZoomAction = 'in' | 'out' | 'reset'

function terminalTheme(): ITerminalOptions['theme'] {
  return {
    background: '#00000000',
    foreground: '#e8e8f0',
    cursor: '#a7f3d0',
    cursorAccent: '#10131d',
    selectionBackground: '#6366f155',
    black: '#20212f',
    red: '#fb7185',
    green: '#86efac',
    yellow: '#fde68a',
    blue: '#93c5fd',
    magenta: '#c4b5fd',
    cyan: '#67e8f9',
    white: '#e5e7eb',
    brightBlack: '#73758a',
    brightRed: '#fda4af',
    brightGreen: '#bbf7d0',
    brightYellow: '#fef3c7',
    brightBlue: '#bfdbfe',
    brightMagenta: '#ddd6fe',
    brightCyan: '#a5f3fc',
    brightWhite: '#ffffff',
  }
}

type ConfigDraft = {
  name: string
  saveFor: TerminalSaveFor
  keepAliveFor: TerminalKeepAliveFor
}

export default function TerminalView({
  initialCommand,
  initialSessionId,
  workingDirectory,
  defaults,
  onBack,
  embedded = false,
}: {
  initialCommand?: string
  initialSessionId?: string
  workingDirectory?: string
  defaults?: TerminalDefaults
  onBack: () => void
  embedded?: boolean
}): ReactNode {
  const hostRef = useRef<HTMLDivElement | null>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const activeSessionIdRef = useRef<string | null>(null)
  const lastAttachedSessionIdRef = useRef<string | null>(null)
  const sessionsRef = useRef<TerminalSessionSummary[]>([])
  const configOpenRef = useRef(false)
  const onBackRef = useRef(onBack)
  const bootRef = useRef({
    initialCommand,
    initialSessionId,
    workingDirectory,
    defaults: defaults ?? readTerminalDefaults(),
  })
  const bootedRef = useRef(false)
  const terminalFontSizeRef = useRef(DEFAULT_TERMINAL_FONT_SIZE)
  // Buffer data that arrives before the terminal has been attached to a session.
  const pendingOutputRef = useRef<Map<string, string>>(new Map())
  const replayingSessionIdRef = useRef<string | null>(null)
  const sessionCwds = useRef<Map<string, string>>(new Map())
  const sessionCommands = useRef<Map<string, string>>(new Map())
  const lastActiveNames = useRef<Map<string, string>>(new Map())
  const inputBuffers = useRef<Map<string, string>>(new Map())
  const cwdRefreshGenerations = useRef<Map<string, number>>(new Map())

  const updateSessionName = useCallback((sessId: string, cwd: string, lastCommand: string) => {
    const newName = formatTerminalSessionName(cwd, lastCommand)
    if (lastActiveNames.current.get(sessId) === newName) return
    lastActiveNames.current.set(sessId, newName)

    void window.tezbar
      .terminalUpdate({
        sessionId: sessId,
        name: newName,
        cwd,
        lastCommand,
      })
      .then((updated) => {
        if (updated) {
          sessionCwds.current.set(sessId, updated.cwd)
          setSessions((prev) =>
            prev.map((s) => (s.sessionId === updated.sessionId ? updated : s)),
          )
        }
      })
  }, [])

  const refreshSessionCwd = useCallback(
    (sessionId: string, lastCommand: string): void => {
      const generation = (cwdRefreshGenerations.current.get(sessionId) ?? 0) + 1
      cwdRefreshGenerations.current.set(sessionId, generation)
      for (const delay of [80, 350, 1_200]) {
        window.setTimeout(() => {
          if (!terminalRef.current) return
          if (cwdRefreshGenerations.current.get(sessionId) !== generation) return
          void window.tezbar.terminalGetCwd(sessionId).then((cwd) => {
            if (!cwd || cwdRefreshGenerations.current.get(sessionId) !== generation) return
            sessionCwds.current.set(sessionId, cwd)
            updateSessionName(sessionId, cwd, lastCommand)
          })
        }, delay)
      }
    },
    [updateSessionName],
  )

  const [terminalReady, setTerminalReady] = useState(false)
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
  const [sessions, setSessions] = useState<TerminalSessionSummary[]>([])
  const [shellName, setShellName] = useState('Shell')
  const [configOpen, setConfigOpen] = useState(false)
  const [isDropdownOpen, setIsDropdownOpen] = useState(false)
  const [terminalFontSize, setTerminalFontSize] = useState(DEFAULT_TERMINAL_FONT_SIZE)
  const [configDraft, setConfigDraft] = useState<ConfigDraft>({
    name: '',
    saveFor: 'week',
    keepAliveFor: '3h',
  })

  const activeSession = useMemo(
    () => sessions.find((session) => session.sessionId === activeSessionId) ?? null,
    [activeSessionId, sessions],
  )
  const activeSessionDirectory = activeSession
    ? terminalDirectoryLabel(activeSession.cwd)
    : shellName
  const activeSessionCommand = normalizeTerminalCommand(
    activeSession?.lastCommand ?? activeSession?.initialCommand,
  )
  const activeSessionTitle = activeSession
    ? formatTerminalSessionName(activeSession.cwd, activeSessionCommand)
    : shellName

  useEffect(() => {
    onBackRef.current = onBack
  }, [onBack])

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId
  }, [activeSessionId])

  useEffect(() => {
    sessionsRef.current = sessions
  }, [sessions])

  useEffect(() => {
    configOpenRef.current = configOpen
  }, [configOpen])

  useEffect(() => {
    if (!configOpen || !activeSession) return
    setConfigDraft({
      name: activeSession.name,
      saveFor: activeSession.saveFor,
      keepAliveFor: activeSession.keepAliveFor,
    })
  }, [activeSession, configOpen])

  const refreshSessions = useCallback(async (): Promise<TerminalSessionSummary[]> => {
    const next = await window.tezbar.terminalList()
    setSessions(next)
    return next
  }, [])

  const fit = useCallback((): void => {
    const terminal = terminalRef.current
    const fitAddon = fitAddonRef.current
    if (!terminal || !fitAddon) return
    try {
      fitAddon.fit()
      const sessionId = activeSessionIdRef.current
      if (sessionId) {
        void window.tezbar.terminalResize(sessionId, terminal.cols, terminal.rows)
      }
    } catch {
      // The host can briefly have zero dimensions while the window hides.
    }
  }, [])

  const updateTerminalZoom = useCallback(
    (action: TerminalZoomAction): void => {
      const current = terminalFontSizeRef.current
      const next =
        action === 'reset'
          ? DEFAULT_TERMINAL_FONT_SIZE
          : Math.min(
              MAX_TERMINAL_FONT_SIZE,
              Math.max(
                MIN_TERMINAL_FONT_SIZE,
                current + (action === 'in' ? TERMINAL_ZOOM_STEP : -TERMINAL_ZOOM_STEP)
              )
            )
      if (next === current) return

      terminalFontSizeRef.current = next
      setTerminalFontSize(next)
      const terminal = terminalRef.current
      if (terminal) terminal.options.fontSize = next
      window.requestAnimationFrame(fit)
    },
    [fit]
  )

  const createSession = useCallback(
    async (opts?: {
      initialCommand?: string
      restoreSessionId?: string
      restoreCommand?: string
      workingDirectory?: string
      name?: string
      saveFor?: TerminalSaveFor
      keepAliveFor?: TerminalKeepAliveFor
    }): Promise<void> => {
      const terminal = terminalRef.current
      if (!terminal) return
      const result = await window.tezbar.terminalCreate({
        cwd: opts?.workingDirectory,
        initialCommand: opts?.initialCommand,
        restoreSessionId: opts?.restoreSessionId,
        restoreCommand: opts?.restoreCommand,
        name: opts?.name,
        saveFor: opts?.saveFor ?? bootRef.current.defaults.saveFor,
        keepAliveFor: opts?.keepAliveFor ?? bootRef.current.defaults.keepAliveFor,
        cols: Math.max(terminal.cols, 80),
        rows: Math.max(terminal.rows, 24),
      })
      setActiveSessionId(result.sessionId)
      setShellName(result.shell.split('/').pop() || result.shell)
      await refreshSessions()
    },
    [refreshSessions],
  )

  const restoreSavedSession = useCallback(
    async (session: TerminalSessionSummary): Promise<void> => {
      if (session.status === 'running') {
        setActiveSessionId(session.sessionId)
        return
      }
      await createSession(terminalSessionRestoreOptions(session))
    },
    [createSession],
  )

  useEffect(() => {
    // Poll sessions list periodically to keep the status dots updated.
    const interval = window.setInterval(() => {
      void refreshSessions()
    }, 3000)
    return () => window.clearInterval(interval)
  }, [refreshSessions])

  useEffect(() => {
    const host = hostRef.current
    if (!host) return

    const terminal = new Terminal({
      allowTransparency: true,
      cursorBlink: true,
      cursorStyle: 'bar',
      convertEol: true,
      fontFamily: '"SFMono-Regular", "SF Mono", Menlo, Monaco, Consolas, monospace',
      fontSize: terminalFontSizeRef.current,
      lineHeight: 1.22,
      // Native history is durable and unbounded on disk. Keep a generous
      // interactive buffer so restored sessions remain browsable in xterm.
      scrollback: 1_000_000,
      theme: terminalTheme(),
    })
    const fitAddon = new FitAddon()
    terminal.loadAddon(fitAddon)
    terminal.open(host)
    terminalRef.current = terminal
    fitAddonRef.current = fitAddon

    let disposed = false
    let resizeFrame = 0
    let wheelZoomAccumulator = 0

    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type === 'keydown' && event.metaKey && event.key === '[') {
        event.preventDefault()
        onBackRef.current()
        return false
      }
      if (event.type === 'keydown' && event.metaKey && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setConfigOpen(true)
        return false
      }
      return true
    })

    const scheduleFit = (): void => {
      cancelAnimationFrame(resizeFrame)
      resizeFrame = requestAnimationFrame(() => {
        if (!disposed) fit()
      })
    }

    const offData = window.tezbar.onTerminalData((event: TerminalDataEvent) => {
      if (
        event.sessionId === activeSessionIdRef.current &&
        event.sessionId !== replayingSessionIdRef.current
      ) {
        terminal.write(event.data)
      } else {
        // Buffer output that arrives before the active session ID is committed
        // (race between native shell startup and React state update).
        const pending = pendingOutputRef.current
        pending.set(event.sessionId, (pending.get(event.sessionId) ?? '') + event.data)
      }
    })
    const offExit = window.tezbar.onTerminalExit((event: TerminalExitEvent) => {
      if (event.sessionId === activeSessionIdRef.current) {
        terminal.writeln('')
        terminal.writeln(`\x1b[90m[process exited with code ${event.exitCode}]\x1b[0m`)
      }
      void refreshSessions()
    })
    const inputDisposable = terminal.onData((data) => {
      const sessionId = activeSessionIdRef.current
      if (!sessionId) return
      void window.tezbar.terminalWrite(sessionId, data)

      const submitCommand = (): void => {
        const buffer = terminal.buffer.active
        const renderedLine =
          buffer.getLine(buffer.baseY + buffer.cursorY)?.translateToString(true) ?? ''
        const buf =
          commandFromTerminalLine(renderedLine) ??
          normalizeTerminalCommand(inputBuffers.current.get(sessionId))
        if (buf) {
          const cmd = buf.split('\n').pop()?.trim() || buf
          sessionCommands.current.set(sessionId, cmd)

          refreshSessionCwd(sessionId, cmd)
        }
        inputBuffers.current.set(sessionId, '')
      }

      if (data === '\x03') {
        inputBuffers.current.set(sessionId, '')
        return
      }
      if (data.startsWith('\x1b')) return

      for (const character of data) {
        if (character === '\r' || character === '\n') {
          submitCommand()
        } else if (character === '\x7f') {
          const buf = inputBuffers.current.get(sessionId) ?? ''
          inputBuffers.current.set(sessionId, buf.slice(0, -1))
        } else if (character >= ' ') {
          const buf = inputBuffers.current.get(sessionId) ?? ''
          inputBuffers.current.set(sessionId, buf + character)
        }
      }
    })

    const onWindowKeyDown = (event: KeyboardEvent): void => {
      const zoomModifier = event.metaKey || event.ctrlKey
      if (zoomModifier && !event.altKey) {
        const zoomAction: TerminalZoomAction | null =
          event.key === '+' || event.key === '=' || event.code === 'NumpadAdd'
            ? 'in'
            : event.key === '-' || event.code === 'NumpadSubtract'
              ? 'out'
              : event.key === '0' || event.code === 'Numpad0'
                ? 'reset'
                : null
        if (zoomAction) {
          event.preventDefault()
          event.stopPropagation()
          updateTerminalZoom(zoomAction)
          return
        }
      }
      if (event.key === 'Escape') {
        if (configOpenRef.current) {
          event.preventDefault()
          event.stopPropagation()
          setConfigOpen(false)
          return
        }
        event.preventDefault()
        event.stopPropagation()
        onBackRef.current()
        return
      }
      if (event.metaKey && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        event.stopPropagation()
        setConfigOpen(true)
        return
      }
      const sessionIndex = terminalSessionShortcutIndex(event)
      if (sessionIndex !== null) {
        const session = sessionsRef.current[sessionIndex]
        if (!session) return
        event.preventDefault()
        event.stopPropagation()
        void restoreSavedSession(session)
      }
    }

    const onZoomWheel = (event: WheelEvent): void => {
      if (!event.metaKey && !event.ctrlKey) {
        wheelZoomAccumulator = 0
        return
      }
      event.preventDefault()
      event.stopPropagation()
      wheelZoomAccumulator += event.deltaY
      if (Math.abs(wheelZoomAccumulator) < 40) return
      updateTerminalZoom(wheelZoomAccumulator < 0 ? 'in' : 'out')
      wheelZoomAccumulator = 0
    }

    const resizeObserver = new ResizeObserver(scheduleFit)
    resizeObserver.observe(host)
    window.addEventListener('keydown', onWindowKeyDown, true)
    host.addEventListener('wheel', onZoomWheel, { passive: false })

    scheduleFit()
    setTerminalReady(true)
    terminal.focus()

    return () => {
      disposed = true
      const attachedSessionId = lastAttachedSessionIdRef.current
      if (attachedSessionId) void window.tezbar.terminalDetach(attachedSessionId)
      lastAttachedSessionIdRef.current = null
      activeSessionIdRef.current = null
      window.removeEventListener('keydown', onWindowKeyDown, true)
      host.removeEventListener('wheel', onZoomWheel)
      cancelAnimationFrame(resizeFrame)
      resizeObserver.disconnect()
      inputDisposable.dispose()
      offData()
      offExit()
      terminal.dispose()
      terminalRef.current = null
      fitAddonRef.current = null
    }
  }, [fit, refreshSessionCwd, refreshSessions, restoreSavedSession, updateTerminalZoom])

  useEffect(() => {
    if (!terminalReady || bootedRef.current) return
    bootedRef.current = true
    void (async () => {
      const initial = bootRef.current
      const existing = await refreshSessions()
      if (initial.initialSessionId) {
        const target = existing.find((session) => session.sessionId === initial.initialSessionId)
        if (target) {
          await restoreSavedSession(target)
          return
        }
      }
      const mostRecentRunning = existing.find((session) => session.status === 'running')
      if (initial.initialCommand || initial.workingDirectory || !mostRecentRunning) {
        await createSession({
          initialCommand: initial.initialCommand,
          workingDirectory: initial.workingDirectory,
          saveFor: initial.defaults.saveFor,
          keepAliveFor: initial.defaults.keepAliveFor,
        })
        return
      }
      setActiveSessionId(mostRecentRunning.sessionId)
    })()
  }, [createSession, refreshSessions, restoreSavedSession, terminalReady])

  useEffect(() => {
    const terminal = terminalRef.current
    if (!terminal || !activeSessionId) return

    let cancelled = false
    const previous = lastAttachedSessionIdRef.current
    if (previous && previous !== activeSessionId) {
      void window.tezbar.terminalDetach(previous)
    }
    lastAttachedSessionIdRef.current = activeSessionId
    replayingSessionIdRef.current = activeSessionId

    terminal.reset()
    void window.tezbar
      .terminalAttach({
        sessionId: activeSessionId,
        cols: Math.max(terminal.cols, 80),
        rows: Math.max(terminal.rows, 24),
      })
      .then(async (result) => {
        if (cancelled) return
        if (!result) {
          replayingSessionIdRef.current = null
          await refreshSessions()
          return
        }
        setShellName(result.shell.split('/').pop() || result.shell)
        const cwd = result.cwd || result.summary?.cwd
        const lastCommand =
          sessionCommands.current.get(activeSessionId) ??
          result.summary?.lastCommand ??
          result.summary?.initialCommand ??
          ''
        sessionCwds.current.set(activeSessionId, cwd)
        if (lastCommand) sessionCommands.current.set(activeSessionId, lastCommand)
        updateSessionName(activeSessionId, cwd, lastCommand)

        if (result.recentOutput) terminal.write(result.recentOutput)
        // Output can arrive while the durable snapshot is in flight. Replay
        // only the suffix that was not already captured in that snapshot.
        const buffered = pendingOutputRef.current.get(activeSessionId)
        if (buffered) {
          const afterSnapshot = terminalOutputAfterSnapshot(result.recentOutput, buffered)
          if (afterSnapshot) terminal.write(afterSnapshot)
          pendingOutputRef.current.delete(activeSessionId)
        }
        replayingSessionIdRef.current = null
        fit()
        terminal.focus()
        await refreshSessions()
      })
      .catch((error: unknown) => {
        replayingSessionIdRef.current = null
        terminal.writeln(
          `\x1b[31mCould not restore terminal: ${error instanceof Error ? error.message : String(error)}\x1b[0m`,
        )
      })

    return () => {
      cancelled = true
      if (replayingSessionIdRef.current === activeSessionId) {
        replayingSessionIdRef.current = null
      }
    }
  }, [activeSessionId, fit, refreshSessions, updateSessionName])

  const saveConfig = async (): Promise<void> => {
    if (!activeSessionId) return
    const updated = await window.tezbar.terminalUpdate({
      sessionId: activeSessionId,
      name: configDraft.name,
      saveFor: configDraft.saveFor,
      keepAliveFor: configDraft.keepAliveFor,
    })
    if (updated) {
      setSessions((prev) =>
        prev.map((session) => (session.sessionId === updated.sessionId ? updated : session)),
      )
      setConfigOpen(false)
    }
  }

  const stopActiveSession = async (): Promise<void> => {
    if (!activeSessionId) return
    await window.tezbar.terminalKill(activeSessionId)
    await refreshSessions()
  }

  return (
    <section
      aria-label="Terminal"
      className={cx(
        'flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in',
        embedded ? 'flex-1' : 'h-[560px] min-h-[560px]',
      )}
    >
      <header className="glass-card relative z-30 flex h-11 shrink-0 items-center justify-between px-5 shadow-[0_18px_45px_rgba(0,0,0,0.32),inset_0_1px_0_rgba(255,255,255,0.04)]">
        <div className="flex min-w-0 flex-1 items-center pr-4" title={activeSessionTitle}>
          <span className="max-w-[42%] shrink-0 truncate text-[13px] font-semibold tracking-tight text-ink-1">
            {activeSessionDirectory}
          </span>
          {activeSessionCommand ? (
            <>
              <span
                aria-hidden
                className="mx-2.5 h-3.5 w-px shrink-0 bg-gradient-to-b from-transparent via-emerald-300/45 to-transparent"
              />
              <span className="min-w-0 truncate font-mono text-[11px] font-medium text-emerald-100/65">
                {activeSessionCommand}
              </span>
            </>
          ) : null}
        </div>
        <div className="no-drag mr-2 flex shrink-0 items-center rounded-lg border border-white/[0.08] bg-black/15 p-0.5">
          <button
            type="button"
            aria-label="Zoom terminal out"
            title="Zoom out (⌘−)"
            disabled={terminalFontSize <= MIN_TERMINAL_FONT_SIZE}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => updateTerminalZoom('out')}
            className="grid h-5 w-5 place-items-center rounded-md text-[13px] text-ink-3 transition hover:bg-white/[0.07] hover:text-ink-1 disabled:cursor-default disabled:opacity-30"
          >
            −
          </button>
          <button
            type="button"
            aria-label="Reset terminal zoom"
            title="Reset zoom (⌘0)"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => updateTerminalZoom('reset')}
            className="h-5 min-w-9 rounded-md px-1 font-mono text-[9px] tabular-nums text-ink-4 transition hover:bg-white/[0.07] hover:text-ink-2"
          >
            {Math.round((terminalFontSize / DEFAULT_TERMINAL_FONT_SIZE) * 100)}%
          </button>
          <button
            type="button"
            aria-label="Zoom terminal in"
            title="Zoom in (⌘+)"
            disabled={terminalFontSize >= MAX_TERMINAL_FONT_SIZE}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => updateTerminalZoom('in')}
            className="grid h-5 w-5 place-items-center rounded-md text-[13px] text-ink-3 transition hover:bg-white/[0.07] hover:text-ink-1 disabled:cursor-default disabled:opacity-30"
          >
            +
          </button>
        </div>
        <div className="relative flex shrink-0 items-center">
          <button
            type="button"
            onClick={() => setIsDropdownOpen(!isDropdownOpen)}
            className="flex items-center gap-2 rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2.5 py-1 text-[11px] font-semibold text-ink-3 transition hover:bg-white/[0.08] hover:text-ink-1"
          >
            <span className={cx(
              "h-1.5 w-1.5 rounded-full",
              activeSession?.status === 'running'
                ? "bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.9)]"
                : "bg-white/20"
            )} />
            <span>Sessions ({sessions.filter(s => s.status === 'running').length})</span>
            <svg viewBox="0 0 20 20" fill="currentColor" className="h-3.5 w-3.5 opacity-60">
              <path fillRule="evenodd" d="M5.22 8.22a.75.75 0 0 1 1.06 0L10 11.94l3.72-3.72a.75.75 0 1 1 1.06 1.06l-4.25 4.25a.75.75 0 0 1-1.06 0L5.22 9.28a.75.75 0 0 1 0-1.06Z" clipRule="evenodd" />
            </svg>
          </button>
          <button
            type="button"
            onClick={() => void createSession()}
            className="grid h-6 w-6 place-items-center rounded-lg border border-white/10 bg-white/[0.04] text-[13px] text-ink-2 transition hover:border-emerald-300/40 hover:text-emerald-200 ml-1.5"
            title="New Session"
          >
            +
          </button>

          {isDropdownOpen && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setIsDropdownOpen(false)} />
              <div className="glass-card absolute right-0 top-[32px] z-50 w-[280px] max-h-[300px] overflow-y-auto rounded-[18px] border border-white/[0.1] bg-[#10131d]/95 p-2 shadow-[0_20px_50px_rgba(0,0,0,0.5)]">
                {sessions.length === 0 ? (
                  <div className="p-3 text-[11px] text-ink-4">No sessions active.</div>
                ) : (
                  sessions.map((session, index) => {
                    const running = session.status === 'running'
                    const active = session.sessionId === activeSessionId
                    return (
                      <button
                        key={session.sessionId}
                        type="button"
                        className={cx(
                          'group relative flex w-full items-start gap-2 rounded-lg px-2.5 py-1.5 text-left transition-all duration-150',
                          active
                            ? 'bg-white/[0.08] text-ink-1 shadow-[inset_0_1px_0_rgba(255,255,255,0.05)]'
                            : 'text-ink-3 hover:bg-white/[0.04] hover:text-ink-1'
                        )}
                        onClick={() => {
                          void restoreSavedSession(session)
                          setIsDropdownOpen(false)
                        }}
                      >
                        {active && (
                          <span className="absolute left-0 top-1.5 bottom-1.5 w-[3px] rounded-r-full bg-gradient-to-b from-emerald-400 to-teal-400 shadow-[0_0_8px_rgba(52,211,153,0.7)]" />
                        )}
                        <span
                          className={cx(
                            'mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full',
                            running
                              ? 'bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.9)]'
                              : 'bg-white/20'
                          )}
                        />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[11.5px] font-semibold">
                            {session.name}
                          </span>
                          <span className="mt-0.5 block truncate font-mono text-[9px] text-ink-4">
                            {session.cwd}
                          </span>
                        </span>
                        <span className="mt-0.5 shrink-0 rounded border border-white/10 bg-black/20 px-1 py-0.5 text-[8.5px] font-semibold text-ink-4">
                          ⌘{index + 1}
                        </span>
                      </button>
                    )
                  })
                )}
              </div>
            </>
          )}
        </div>
      </header>

      <div className="glass-card no-drag relative z-10 flex min-h-0 flex-1 flex-col overflow-hidden rounded-[24px] border border-white/[0.08] bg-[#10131d]/92 shadow-[0_24px_60px_rgba(0,0,0,0.36),inset_0_1px_0_rgba(255,255,255,0.035)] p-3">
        <div ref={hostRef} className="terminal-host h-full w-full overflow-hidden" />
      </div>

      <div className="glass-card relative z-20 flex h-10 shrink-0 items-center px-5 shadow-[0_18px_45px_rgba(0,0,0,0.32),inset_0_1px_0_rgba(255,255,255,0.035)]">
        <HintBar>
          <Hint label="Configure" keys={<><Kbd>⌘</Kbd><Kbd>K</Kbd></>} />
          <Hint label="Switch sessions" keys={<><Kbd>⌘</Kbd><Kbd>1-9</Kbd></>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
          <Hint label="Back" keys={<><Kbd>⌘</Kbd><Kbd>[</Kbd></>} />
        </HintBar>
      </div>

      {configOpen && activeSession ? (
        <div className="absolute inset-0 z-40 flex items-start justify-center bg-black/35 px-8 py-12 backdrop-blur-sm">
          <form
            className="w-full max-w-[520px] rounded-[18px] border border-white/[0.1] bg-[#10131d]/95 p-4 shadow-[0_24px_80px_rgba(0,0,0,0.5)]"
            onSubmit={(event) => {
              event.preventDefault()
              void saveConfig()
            }}
          >
            <div className="mb-4 flex items-start justify-between gap-4">
              <div>
                <h2 className="text-[14px] font-semibold text-ink-1">Terminal Session</h2>
                <p className="mt-1 text-[11px] text-ink-4">
                  Save the session label and control how long the process stays alive after you leave.
                </p>
              </div>
              <button
                type="button"
                className="rounded-tezbar-chip px-2 py-1 text-[11px] text-ink-4 transition hover:bg-white/[0.06] hover:text-ink-1"
                onClick={() => setConfigOpen(false)}
              >
                Esc
              </button>
            </div>
            <label className="mb-3 block" htmlFor="terminal-session-name">
              <span className="mb-1.5 block text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-3">
                Name
              </span>
              <TextField
                id="terminal-session-name"
                value={configDraft.name}
                onChange={(event) =>
                  setConfigDraft((prev) => ({ ...prev, name: event.target.value }))
                }
                placeholder="code/aml pnpm dev"
              />
            </label>
            <div className="grid grid-cols-2 gap-3">
              <label className="block" htmlFor="terminal-session-save-for">
                <span className="mb-1.5 block text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-3">
                  Save session for
                </span>
                <SelectField
                  id="terminal-session-save-for"
                  value={configDraft.saveFor}
                  onChange={(event) =>
                    setConfigDraft((prev) => ({
                      ...prev,
                      saveFor: event.target.value as TerminalSaveFor,
                    }))
                  }
                >
                  {SAVE_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </SelectField>
              </label>
              <label className="block" htmlFor="terminal-session-keep-alive">
                <span className="mb-1.5 block text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-3">
                  Keep process running
                </span>
                <SelectField
                  id="terminal-session-keep-alive"
                  value={configDraft.keepAliveFor}
                  onChange={(event) =>
                    setConfigDraft((prev) => ({
                      ...prev,
                      keepAliveFor: event.target.value as TerminalKeepAliveFor,
                    }))
                  }
                >
                  {KEEP_ALIVE_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </SelectField>
              </label>
            </div>
            <div className="mt-5 flex items-center justify-between gap-3">
              <button
                type="button"
                className="rounded-tezbar-chip border border-rose-400/20 bg-rose-400/10 px-3 py-1.5 text-[11px] font-semibold text-rose-200 transition hover:border-rose-300/40"
                onClick={() => {
                  void stopActiveSession()
                  setConfigOpen(false)
                }}
              >
                Stop process
              </button>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  className="rounded-tezbar-chip border border-white/10 px-3 py-1.5 text-[11px] font-semibold text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1"
                  onClick={() => setConfigOpen(false)}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="rounded-tezbar-chip border border-emerald-300/35 bg-emerald-400/15 px-3 py-1.5 text-[11px] font-semibold text-emerald-100 transition hover:bg-emerald-400/20"
                >
                  Save
                </button>
              </div>
            </div>
          </form>
        </div>
      ) : null}
    </section>
  )
}
