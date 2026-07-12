import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Terminal, type ITerminalOptions } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { cx, Hint, HintBar, Kbd, SelectField, TextField } from './ui/primitives'
import { readTerminalDefaults, type TerminalDefaults } from './terminalPreferences'
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

function formatDynamicSessionName(cwd: string, lastCommand: string): string {
  const parts = cwd.split('/').filter(Boolean)
  const codeIndex = parts.lastIndexOf('code')
  const compactPath =
    codeIndex >= 0 && parts[codeIndex + 1]
      ? `code/${parts[codeIndex + 1]}`
      : parts[parts.length - 1] || '~'

  const command = lastCommand.trim().replace(/\s+/g, ' ')
  return command ? `${compactPath} ${command}` : compactPath
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
  // Buffer data that arrives before the terminal has been attached to a session.
  const pendingOutputRef = useRef<Map<string, string>>(new Map())
  const sessionCwds = useRef<Map<string, string>>(new Map())
  const sessionCommands = useRef<Map<string, string>>(new Map())
  const lastActiveNames = useRef<Map<string, string>>(new Map())
  const inputBuffers = useRef<Map<string, string>>(new Map())

  const updateSessionName = useCallback((sessId: string, cwd: string, lastCommand: string) => {
    const newName = formatDynamicSessionName(cwd, lastCommand)
    if (lastActiveNames.current.get(sessId) === newName) return
    lastActiveNames.current.set(sessId, newName)

    void window.tezbar.terminalUpdate({
      sessionId: sessId,
      name: newName,
      cwd,
      lastCommand,
    }).then((updated) => {
      if (updated) {
        setSessions((prev) =>
          prev.map((s) => (s.sessionId === updated.sessionId ? updated : s))
        )
      }
    })
  }, [])

  const [terminalReady, setTerminalReady] = useState(false)
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
  const [sessions, setSessions] = useState<TerminalSessionSummary[]>([])
  const [shellName, setShellName] = useState('Shell')
  const [configOpen, setConfigOpen] = useState(false)
  const [isDropdownOpen, setIsDropdownOpen] = useState(false)
  const [configDraft, setConfigDraft] = useState<ConfigDraft>({
    name: '',
    saveFor: 'week',
    keepAliveFor: '3h',
  })

  const activeSession = useMemo(
    () => sessions.find((session) => session.sessionId === activeSessionId) ?? null,
    [activeSessionId, sessions],
  )

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

  const createSession = useCallback(
    async (opts?: {
      initialCommand?: string
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
      await createSession({
        workingDirectory: session.cwd,
        name: session.name,
        saveFor: session.saveFor,
        keepAliveFor: session.keepAliveFor,
      })
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
      fontSize: 12,
      lineHeight: 1.22,
      scrollback: 5000,
      theme: terminalTheme(),
    })
    const fitAddon = new FitAddon()
    terminal.loadAddon(fitAddon)
    terminal.open(host)
    terminalRef.current = terminal
    fitAddonRef.current = fitAddon

    let disposed = false
    let resizeFrame = 0

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
      if (event.sessionId === activeSessionIdRef.current) {
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
        const buf = (inputBuffers.current.get(sessionId) ?? '').trim()
        if (buf) {
          const cmd = buf.split('\n').pop()?.trim() || buf
          sessionCommands.current.set(sessionId, cmd)

          // Detect cd commands to update tracked CWD
          const cdMatch = cmd.match(/^cd(?:\s+(.+))?$/)
          if (cdMatch) {
            const currentCwd = sessionCwds.current.get(sessionId) || ''
            let target = (cdMatch[1] ?? '~').replace(/^['"]|['"]$/g, '').trim()
            if (target === '~' || target === '') {
              target = `/Users/${currentCwd.split('/')[2] || 'user'}`
            } else if (target.startsWith('~/')) {
              target = `/Users/${currentCwd.split('/')[2] || 'user'}/${target.slice(2)}`
            } else if (!target.startsWith('/')) {
              target = `${currentCwd}/${target}`.replace(/\/+/g, '/')
            }
            // Resolve .. in path
            const resolved = target.split('/').reduce<string[]>((acc, part) => {
              if (part === '..') acc.pop()
              else if (part && part !== '.') acc.push(part)
              return acc
            }, [])
            sessionCwds.current.set(sessionId, '/' + resolved.join('/'))
          }

          const cwd = sessionCwds.current.get(sessionId) || ''
          if (cwd) updateSessionName(sessionId, cwd, cmd)
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
      if (
        event.altKey &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.shiftKey &&
        /^[1-9]$/.test(event.key)
      ) {
        const index = Number(event.key) - 1
        const session = sessionsRef.current[index]
        if (!session) return
        event.preventDefault()
        event.stopPropagation()
        void restoreSavedSession(session)
      }
    }

    const resizeObserver = new ResizeObserver(scheduleFit)
    resizeObserver.observe(host)
    window.addEventListener('keydown', onWindowKeyDown, true)

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
      cancelAnimationFrame(resizeFrame)
      resizeObserver.disconnect()
      inputDisposable.dispose()
      offData()
      offExit()
      terminal.dispose()
      terminalRef.current = null
      fitAddonRef.current = null
    }
  }, [fit, refreshSessions, restoreSavedSession, updateSessionName])

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
  }, [createSession, refreshSessions, terminalReady])

  useEffect(() => {
    const terminal = terminalRef.current
    if (!terminal || !activeSessionId) return

    let cancelled = false
    const previous = lastAttachedSessionIdRef.current
    if (previous && previous !== activeSessionId) {
      void window.tezbar.terminalDetach(previous)
    }
    lastAttachedSessionIdRef.current = activeSessionId

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
          await refreshSessions()
          return
        }
        setShellName(result.shell.split('/').pop() || result.shell)
        const cwd = result.summary?.cwd ?? result.cwd
        const lastCommand =
          sessionCommands.current.get(activeSessionId) ??
          result.summary?.lastCommand ??
          result.summary?.initialCommand ??
          ''
        sessionCwds.current.set(activeSessionId, cwd)
        if (lastCommand) sessionCommands.current.set(activeSessionId, lastCommand)
        updateSessionName(activeSessionId, cwd, lastCommand)

        if (result.recentOutput) terminal.write(result.recentOutput)
        // Flush output that arrived before the session ID was committed to state.
        const buffered = pendingOutputRef.current.get(activeSessionId)
        if (buffered) {
          terminal.write(buffered)
          pendingOutputRef.current.delete(activeSessionId)
        }
        fit()
        terminal.focus()
        await refreshSessions()
      })
      .catch((error: unknown) => {
        terminal.writeln(
          `\x1b[31mCould not restore terminal: ${error instanceof Error ? error.message : String(error)}\x1b[0m`,
        )
      })

    return () => {
      cancelled = true
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
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold tracking-tight text-ink-1" title={activeSession?.name ?? shellName}>
          {activeSession?.name ?? shellName}
        </span>
        <div className="relative flex items-center">
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
                          ⌥{index + 1}
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

      <div className="glass-card relative z-10 flex min-h-0 flex-1 flex-col overflow-hidden rounded-[24px] border border-white/[0.08] bg-[#10131d]/92 shadow-[0_24px_60px_rgba(0,0,0,0.36),inset_0_1px_0_rgba(255,255,255,0.035)] p-3">
        <div ref={hostRef} className="terminal-host h-full w-full overflow-hidden" />
      </div>

      <div className="glass-card relative z-20 flex h-10 shrink-0 items-center px-5 shadow-[0_18px_45px_rgba(0,0,0,0.32),inset_0_1px_0_rgba(255,255,255,0.035)]">
        <HintBar>
          <Hint label="Configure" keys={<><Kbd>⌘</Kbd><Kbd>K</Kbd></>} />
          <Hint label="Switch sessions" keys={<><Kbd>⌥</Kbd><Kbd>1-9</Kbd></>} />
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
