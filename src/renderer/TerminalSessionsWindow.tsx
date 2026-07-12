import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { compactTerminalPath, type TerminalSessionSummary } from '../shared/terminal'
import { cx } from './ui/primitives'

function sessionSubtitle(session: TerminalSessionSummary): string {
  const command = session.lastCommand ?? session.initialCommand
  const commandSuffix = command ? ` · ${command}` : ''
  return `${compactTerminalPath(session.cwd)}${commandSuffix}`
}

function sessionAge(ts: number): string {
  const delta = Math.max(0, Date.now() - ts)
  const minutes = Math.floor(delta / 60_000)
  if (minutes < 1) return 'now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  return `${Math.floor(hours / 24)}d ago`
}

export default function TerminalSessionsWindow(): ReactNode {
  const [sessions, setSessions] = useState<TerminalSessionSummary[]>([])
  const sessionsRef = useRef<TerminalSessionSummary[]>([])

  const runningCount = useMemo(
    () => sessions.filter((session) => session.status === 'running').length,
    [sessions],
  )

  useEffect(() => {
    sessionsRef.current = sessions
  }, [sessions])

  useEffect(() => {
    let cancelled = false
    const load = async (): Promise<void> => {
      try {
        const next = await window.tezbar.terminalList()
        if (!cancelled) setSessions(next)
      } catch {
        if (!cancelled) setSessions([])
      }
    }

    void load()
    const interval = window.setInterval(() => {
      void load()
    }, 2000)

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.preventDefault()
        void window.tezbar.terminalSessionsHide()
        return
      }
      if (
        event.altKey &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.shiftKey &&
        /^[1-9]$/.test(event.key)
      ) {
        const session = sessionsRef.current[Number(event.key) - 1]
        if (!session) return
        event.preventDefault()
        void window.tezbar.terminalSessionsAction({
          type: 'select',
          sessionId: session.sessionId,
        })
      }
    }
    window.addEventListener('keydown', onKeyDown, true)

    return () => {
      cancelled = true
      window.clearInterval(interval)
      window.removeEventListener('keydown', onKeyDown, true)
    }
  }, [])

  return (
    <main className="h-screen w-screen overflow-hidden bg-transparent p-0">
      <section className="glass-card flex h-full w-full flex-col overflow-hidden rounded-[24px] border border-white/[0.1] bg-[#0a0f17]/96 p-3 shadow-[0_24px_70px_rgba(0,0,0,0.56),inset_0_1px_0_rgba(255,255,255,0.055)] backdrop-blur-2xl">
        <div className="mb-2 flex items-center justify-between gap-2 px-1">
          <div className="min-w-0">
            <p className="text-[10px] font-semibold uppercase tracking-[0.16em] text-emerald-300/80">
              Terminal Sessions
            </p>
            <p className="truncate text-[10.5px] text-ink-4">{runningCount} running</p>
          </div>
          <button
            type="button"
            className="grid h-7 w-7 shrink-0 place-items-center rounded-tezbar-chip border border-white/10 bg-white/[0.04] text-ink-2 transition hover:border-emerald-300/40 hover:text-emerald-200"
            aria-label="New terminal session"
            onClick={() => {
              void window.tezbar.terminalSessionsAction({ type: 'new' })
            }}
          >
            +
          </button>
        </div>
        <div className="min-h-0 flex-1 space-y-1 overflow-y-auto pr-1">
          {sessions.length === 0 ? (
            <div className="rounded-tezbar-row border border-white/[0.06] bg-white/[0.03] px-3 py-3 text-[11px] text-ink-4">
              Starting shell...
            </div>
          ) : (
            sessions.slice(0, 9).map((session, index) => {
              const running = session.status === 'running'
              return (
                <button
                  key={session.sessionId}
                  type="button"
                  className="group relative flex w-full items-start gap-2 rounded-tezbar-row px-2.5 py-2 text-left text-ink-3 transition hover:bg-white/[0.05] hover:text-ink-1"
                  onClick={() => {
                    void window.tezbar.terminalSessionsAction({
                      type: 'select',
                      sessionId: session.sessionId,
                    })
                  }}
                >
                  <span
                    className={cx(
                      'mt-1 h-2 w-2 shrink-0 rounded-full',
                      running
                        ? 'bg-emerald-300 shadow-[0_0_12px_rgba(110,231,183,0.8)]'
                        : 'bg-white/20',
                    )}
                    aria-label={running ? 'Running' : 'Exited'}
                  />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[12px] font-semibold">
                      {session.name}
                    </span>
                    <span className="mt-0.5 block truncate font-mono text-[10px] text-ink-4">
                      {sessionSubtitle(session)}
                    </span>
                    <span className="mt-1 block text-[9.5px] text-ink-4">
                      {running ? 'running' : `saved ${sessionAge(session.updatedAt)}`}
                    </span>
                  </span>
                  <span className="mt-0.5 shrink-0 rounded-md border border-white/10 bg-black/20 px-1.5 py-0.5 text-[9px] font-semibold text-ink-4">
                    ⌥{index + 1}
                  </span>
                </button>
              )
            })
          )}
        </div>
      </section>
    </main>
  )
}
