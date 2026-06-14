import { useCallback, useEffect, useRef, useState } from 'react'
import type { PermissionsSnapshot, PermissionState, PermissionStatus } from '../shared/permissions'
import { Button, Hint, HintBar, Kbd, Message, ViewHeader, cx } from './ui/primitives'

const STATE_LABEL: Record<PermissionState, string> = {
  granted: 'Granted',
  denied: 'Denied',
  restricted: 'Restricted',
  'not-determined': 'Not granted',
  unsupported: 'N/A',
}

function stateTone(state: PermissionState): string {
  switch (state) {
    case 'granted':
      return 'text-emerald-300 bg-emerald-300/10 ring-emerald-300/20'
    case 'denied':
    case 'restricted':
      return 'text-rose-300 bg-rose-300/10 ring-rose-300/20'
    case 'not-determined':
      return 'text-amber-300 bg-amber-300/10 ring-amber-300/20'
    default:
      return 'text-ink-3 bg-white/[0.04] ring-white/10'
  }
}

export default function PermissionsView({
  onBack,
  nativeWindow = false,
}: {
  onBack: () => void
  nativeWindow?: boolean
}): JSX.Element {
  const rootRef = useRef<HTMLDivElement>(null)
  const [snapshot, setSnapshot] = useState<PermissionsSnapshot | null>(null)
  const [pending, setPending] = useState<string | null>(null)
  const [banner, setBanner] = useState<{ tone: 'success' | 'error' | 'info'; text: string } | null>(
    null
  )

  const reload = useCallback(async () => {
    try {
      const snap = await window.tezbar.getPermissions()
      setSnapshot(snap)
    } catch (error) {
      setBanner({
        tone: 'error',
        text: `Could not read permissions: ${error instanceof Error ? error.message : String(error)}`,
      })
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  useEffect(() => {
    rootRef.current?.focus()
  }, [])

  useEffect(() => {
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        onBack()
        return
      }
      if (e.key.toLowerCase() === 'r' && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault()
        e.stopPropagation()
        void reload()
      }
    }
    window.addEventListener('keydown', onEsc, true)
    return () => window.removeEventListener('keydown', onEsc, true)
  }, [onBack, reload])

  const request = async (status: PermissionStatus): Promise<void> => {
    setPending(status.descriptor.id)
    setBanner(null)
    try {
      const next = await window.tezbar.requestPermission(status.descriptor.id)
      await reload()
      if (next.state === 'granted') {
        setBanner({ tone: 'success', text: `${next.descriptor.title} is now granted.` })
      } else {
        setBanner({
          tone: 'info',
          text: `${next.descriptor.title}: ${next.descriptor.remediation}`,
        })
      }
    } catch (error) {
      setBanner({
        tone: 'error',
        text: `Could not request ${status.descriptor.title}: ${error instanceof Error ? error.message : String(error)}`,
      })
    } finally {
      setPending(null)
    }
  }

  const content = (
    <>
      {banner ? (
        <div className="mb-3 rounded-tezbar-row border border-white/10 bg-white/[0.035] px-3 py-2">
          <Message tone={banner.tone}>{banner.text}</Message>
        </div>
      ) : null}
      {!snapshot ? (
        <div className="flex h-full min-h-[220px] items-center justify-center text-[12px] text-ink-3">
          Loading...
        </div>
      ) : (
        <div className="min-h-0">
          <ul className="min-w-0 space-y-2">
            {snapshot.statuses.map((status) => {
              const needsAction = status.state !== 'granted' && status.state !== 'unsupported'

              return (
                <li
                  key={status.descriptor.id}
                  className="rounded-tezbar-row border border-white/[0.07] bg-white/[0.025] px-3.5 py-3 transition hover:border-white/[0.12] hover:bg-white/[0.04]"
                >
                  <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4">
                    <div className="min-w-0">
                      <div className="flex min-w-0 items-center gap-2">
                        <span className="truncate text-[13px] font-semibold text-ink-1">
                          {status.descriptor.title}
                        </span>
                        <span
                          className={cx(
                            'rounded-tezbar-chip px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.1em] ring-1 ring-inset',
                            stateTone(status.state)
                          )}
                        >
                          {STATE_LABEL[status.state]}
                        </span>
                      </div>
                      <p className="mt-1.5 text-[12px] leading-snug text-ink-3">
                        {status.descriptor.rationale}
                      </p>
                      {needsAction ? (
                        <p className="mt-1 text-[11.5px] leading-snug text-ink-4">
                          {status.descriptor.remediation}
                        </p>
                      ) : null}
                    </div>

                    {needsAction ? (
                      <Button
                        variant="primary"
                        onClick={() => void request(status)}
                        disabled={pending === status.descriptor.id}
                        className="min-w-[112px]"
                      >
                        {pending === status.descriptor.id ? 'Opening...' : 'Request'}
                      </Button>
                    ) : (
                      <span className="min-w-[112px] text-center text-[11.5px] font-medium text-ink-4">
                        Ready
                      </span>
                    )}
                  </div>
                </li>
              )
            })}
          </ul>
        </div>
      )}
    </>
  )

  if (nativeWindow) {
    return (
      <div
        ref={rootRef}
        tabIndex={-1}
        role="application"
        aria-label="Permissions"
        className="flex h-full min-h-0 w-full flex-col bg-[#1e1f2e] outline-none animate-tezbar-scale-in"
      >
        <header className="shrink-0 border-b border-white/[0.07] px-5 py-3">
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onBack}
              aria-label="Back"
              className="flex h-7 w-7 items-center justify-center rounded-tezbar-chip text-ink-3 transition hover:bg-white/[0.06] hover:text-ink-1"
            >
              <svg
                width="15"
                height="15"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.5"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden
              >
                <path d="m15 18-6-6 6-6" />
              </svg>
            </button>
            <div className="min-w-0">
              <h1 className="text-[15px] font-semibold text-ink-1">Permissions</h1>
              <p className="mt-0.5 text-[11.5px] text-ink-4">
                System access used by automation, voice, calendar, and screen-aware features.
              </p>
            </div>
            <Button variant="quiet" className="ml-auto" onClick={() => void reload()}>
              Refresh
            </Button>
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{content}</main>

        <footer className="shrink-0 border-t border-white/[0.07] px-5 py-2">
          <HintBar>
            <Hint label="Refresh" keys={<Kbd>R</Kbd>} />
            <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
          </HintBar>
        </footer>
      </div>
    )
  }

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="application"
      aria-label="Permissions"
      className="flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3 animate-tezbar-scale-in">
        <ViewHeader
          title="Permissions"
          onBack={onBack}
          trailing={
            <Button variant="quiet" onClick={() => void reload()}>
              Refresh
            </Button>
          }
        />
      </div>

      <div className="glass-card min-h-0 flex-1 overflow-y-auto px-4 py-3 pr-[calc(0.5rem+2px)] animate-tezbar-scale-in">
        {content}
      </div>

      <div className="glass-card shrink-0 px-4 py-2 animate-tezbar-scale-in">
        <HintBar>
          <Hint label="Refresh" keys={<Kbd>R</Kbd>} />
          <Hint label="Back" keys={<Kbd>Esc</Kbd>} />
        </HintBar>
      </div>
    </div>
  )
}
