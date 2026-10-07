import { useEffect, useRef, useState } from 'react'
import { Markdown } from './ui/Markdown'
import { Button, Hint, HintBar, Kbd, ViewHeader } from './ui/primitives'
import {
  CURRENT_APP_VERSION,
  fetchReleaseNotes,
  LAST_RELEASE_NOTES_VERSION_KEY,
  type ReleaseNotes,
} from './releaseNotes'

export default function ReleaseNotesView({
  version = CURRENT_APP_VERSION,
  initialNotes,
  onBack,
  onRead,
}: {
  version?: string
  initialNotes?: ReleaseNotes
  onBack: () => void
  onRead?: () => void
}): JSX.Element {
  const root = useRef<HTMLDivElement>(null)
  const [notes, setNotes] = useState(initialNotes)
  const [error, setError] = useState(false)
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    root.current?.focus()
  }, [])

  useEffect(() => {
    if (initialNotes) return
    let active = true
    const controller = new AbortController()
    const timer = window.setTimeout(() => controller.abort(), 10_000)
    setError(false)
    void fetchReleaseNotes(version, controller.signal)
      .then((release) => {
        if (active) setNotes(release)
      })
      .catch(() => {
        if (active) setError(true)
      })
      .finally(() => window.clearTimeout(timer))
    return () => {
      active = false
      controller.abort()
      window.clearTimeout(timer)
    }
  }, [version, initialNotes, attempt])

  useEffect(() => {
    if (!notes || notes.version !== CURRENT_APP_VERSION || !notes.body.trim()) return
    try {
      window.localStorage.setItem(LAST_RELEASE_NOTES_VERSION_KEY, notes.version)
    } catch {
      // The launcher can still mark the notes read for this session.
    }
    onRead?.()
  }, [notes, onRead])

  return (
    <div
      ref={root}
      tabIndex={-1}
      aria-label="Release Notes"
      onKeyDown={(event) => {
        if (event.key !== 'Escape') return
        event.preventDefault()
        event.stopPropagation()
        onBack()
      }}
      onClick={(event) => {
        const link = (event.target as Element).closest('a[href]')
        const href = link?.getAttribute('href')
        if (href && /^https?:\/\//i.test(href)) {
          event.preventDefault()
          void window.tezbar.openReleasePage(href)
        }
      }}
      className="no-drag flex h-full min-h-0 w-full flex-col gap-2 outline-none animate-tezbar-scale-in"
    >
      <div className="glass-card shrink-0 px-4 py-3">
        <ViewHeader
          title="What’s new in Tezbar"
          onBack={onBack}
          trailing={
            <span className="rounded-tezbar-chip border border-white/10 px-2 py-1 text-xs text-ink-2">
              v{version}
            </span>
          }
        />
        <p className="mt-2 text-xs text-ink-3">Release notes for your Tezbar version.</p>
      </div>
      <div className="glass-card min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {error ? (
          <div role="status" className="space-y-3 text-sm text-ink-3">
            <p>Couldn’t load the Tezbar release notes. Please try again.</p>
            <Button onClick={() => setAttempt((value) => value + 1)}>Retry</Button>
          </div>
        ) : notes ? (
          notes.body.trim() ? (
            <Markdown text={notes.body} />
          ) : (
            <p className="text-sm text-ink-3">No release notes were published for this version.</p>
          )
        ) : (
          <div role="status" aria-label="Loading release notes" className="animate-pulse space-y-3">
            <div className="h-4 w-2/3 rounded bg-white/10" />
            <div className="h-4 rounded bg-white/5" />
            <div className="h-4 w-4/5 rounded bg-white/5" />
          </div>
        )}
      </div>
      <HintBar className="shrink-0 px-2 py-1">
        <Hint keys={<Kbd>ESC</Kbd>} label="Back" />
      </HintBar>
    </div>
  )
}
