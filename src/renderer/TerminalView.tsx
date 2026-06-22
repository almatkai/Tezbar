import { useEffect, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { cx } from './ui/primitives'
import {
  compactTerminalPath,
  type TerminalDataEvent,
  type TerminalExitEvent,
} from '../shared/terminal'

export default function TerminalView({
  initialCommand,
  workingDirectory,
  onBack,
  embedded = false,
}: {
  initialCommand?: string
  workingDirectory?: string
  onBack: () => void
  embedded?: boolean
}): JSX.Element {
  const hostRef = useRef<HTMLDivElement | null>(null)
  const onBackRef = useRef(onBack)
  const [shellName, setShellName] = useState('Shell')
  const [cwd, setCwd] = useState('')

  useEffect(() => {
    onBackRef.current = onBack
  }, [onBack])

  useEffect(() => {
    const host = hostRef.current
    if (!host) return

    const terminal = new Terminal({
      cursorBlink: true,
      cursorStyle: 'bar',
      convertEol: true,
      fontFamily: '"SFMono-Regular", "SF Mono", Menlo, Monaco, Consolas, monospace',
      fontSize: 12,
      lineHeight: 1.22,
      scrollback: 5000,
      theme: {
        background: '#151621',
        foreground: '#e8e8f0',
        cursor: '#a7f3d0',
        cursorAccent: '#151621',
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
      },
    })
    const fitAddon = new FitAddon()
    terminal.loadAddon(fitAddon)
    terminal.open(host)

    let sessionId: string | null = null
    let disposed = false
    let resizeFrame = 0
    const pendingData: TerminalDataEvent[] = []
    const pendingExits: TerminalExitEvent[] = []

    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type === 'keydown' && event.metaKey && event.key === '[') {
        event.preventDefault()
        onBackRef.current()
        return false
      }
      return true
    })

    const fit = (): void => {
      cancelAnimationFrame(resizeFrame)
      resizeFrame = requestAnimationFrame(() => {
        if (disposed) return
        try {
          fitAddon.fit()
          if (sessionId) {
            void window.tezbar.terminalResize(sessionId, terminal.cols, terminal.rows)
          }
        } catch {
          // The host can briefly have zero dimensions while the window hides.
        }
      })
    }

    const handleExit = (event: TerminalExitEvent): void => {
      if (event.sessionId !== sessionId) return
      terminal.writeln('')
      terminal.writeln(`\x1b[90m[process exited with code ${event.exitCode}]\x1b[0m`)
      setTimeout(() => {
        if (!disposed) onBackRef.current()
      }, 350)
    }
    const offData = window.tezbar.onTerminalData((event) => {
      if (!sessionId) {
        pendingData.push(event)
        return
      }
      if (event.sessionId === sessionId) {
        terminal.write(event.data)
      }
    })
    const offExit = window.tezbar.onTerminalExit((event) => {
      if (!sessionId) {
        pendingExits.push(event)
        return
      }
      handleExit(event)
    })
    const inputDisposable = terminal.onData((data) => {
      if (sessionId) void window.tezbar.terminalWrite(sessionId, data)
    })

    const onWindowKeyDown = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        onBackRef.current()
      }
    }
    window.addEventListener('keydown', onWindowKeyDown, true)
    const resizeObserver = new ResizeObserver(fit)
    resizeObserver.observe(host)

    fit()
    void window.tezbar
      .terminalCreate({
        cwd: workingDirectory,
        initialCommand,
        cols: Math.max(terminal.cols, 80),
        rows: Math.max(terminal.rows, 24),
      })
      .then((result) => {
        if (disposed) {
          void window.tezbar.terminalKill(result.sessionId)
          return
        }
        sessionId = result.sessionId
        setShellName(result.shell.split('/').pop() || result.shell)
        setCwd(result.cwd)
        for (const event of pendingData) {
          if (event.sessionId === sessionId) terminal.write(event.data)
        }
        for (const event of pendingExits) {
          handleExit(event)
        }
        fit()
        terminal.focus()
      })
      .catch((error: unknown) => {
        terminal.writeln(`\x1b[31mCould not start terminal: ${error instanceof Error ? error.message : String(error)}\x1b[0m`)
      })

    return () => {
      disposed = true
      window.removeEventListener('keydown', onWindowKeyDown, true)
      cancelAnimationFrame(resizeFrame)
      resizeObserver.disconnect()
      inputDisposable.dispose()
      offData()
      offExit()
      terminal.dispose()
      if (sessionId) void window.tezbar.terminalKill(sessionId)
    }
  }, [initialCommand, workingDirectory])

  return (
    <section
      aria-label="Terminal"
      className={cx(
        'glass-card flex w-full flex-col overflow-hidden',
        embedded ? 'flex-1' : 'h-[560px] min-h-[560px]',
      )}
    >
      {!embedded ? (
        <header className="flex h-10 shrink-0 items-center gap-3 border-b border-white/[0.08] px-3">
          <button
            type="button"
            className="rounded-tezbar-chip border border-white/10 bg-white/[0.04] px-2 py-1 text-[10px] font-semibold text-ink-2 transition hover:border-white/20 hover:text-ink-1"
            onClick={onBack}
          >
            Back
          </button>
          <span className="inline-flex items-center gap-1.5 rounded-tezbar-chip border border-emerald-400/25 bg-emerald-400/10 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-emerald-200">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-300" />
            Direct shell
          </span>
          <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-ink-4" title={cwd}>
            {shellName}{cwd ? ` · ${compactTerminalPath(cwd)}` : ''}
          </span>
          <span className="shrink-0 font-mono text-[9px] text-ink-4">⌘[ back</span>
        </header>
      ) : null}
      <div className="min-h-0 flex-1 bg-[#151621] p-2">
        <div ref={hostRef} className="h-full w-full overflow-hidden" />
      </div>
    </section>
  )
}
