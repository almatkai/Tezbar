import { useEffect, useRef, useState } from 'react'
import { Markdown } from './ui/Markdown'
import { Button } from './ui/primitives'
import {
  CURRENT_APP_VERSION,
  fetchReleaseNotes,
  LAST_RELEASE_NOTES_VERSION_KEY,
  shouldShowPostUpdateNotes,
  type ReleaseNotes,
} from './releaseNotes'

export function ReleaseNotesDialog({
  version,
  initialNotes,
  afterUpdate = false,
  onClose,
}: {
  version: string
  initialNotes?: ReleaseNotes
  afterUpdate?: boolean
  onClose: () => void
}): JSX.Element {
  const dialog = useRef<HTMLDialogElement>(null)
  const [notes, setNotes] = useState(initialNotes)
  const [error, setError] = useState(false)
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null
    const element = dialog.current
    element?.showModal()
    return () => {
      element?.close()
      previous?.focus()
    }
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
    return () => {
      active = false
      controller.abort()
      window.clearTimeout(timer)
    }
  }, [version, initialNotes, attempt])

  return (
    <dialog
      ref={dialog}
      aria-labelledby="release-notes-title"
      onCancel={onClose}
      onKeyDown={(event) => {
        event.stopPropagation()
        if (event.key !== 'Tab') return
        const controls = Array.from(
          event.currentTarget.querySelectorAll<HTMLElement>(
            'button:not([disabled]), a[href], [tabindex="0"]'
          )
        ).filter((element) => element.getClientRects().length > 0)
        const first = controls[0]
        const last = controls[controls.length - 1]
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault()
          last?.focus()
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault()
          first?.focus()
        }
      }}
      onClick={(event) => {
        const link = (event.target as Element).closest('a[href]')
        const href = link?.getAttribute('href')
        if (href && /^https?:\/\//i.test(href)) {
          event.preventDefault()
          void window.tezbar.openReleasePage(href)
        }
      }}
      className="no-drag m-auto w-[calc(100%_-_2rem)] max-w-xl overflow-hidden rounded-tezbar-row border border-white/10 bg-glass-panel p-0 text-ink-1 shadow-xl backdrop:bg-black/60"
    >
      <header className="border-b border-white/10 px-6 py-5">
        <p className="mb-1 text-[11px] font-medium uppercase tracking-widest text-ink-3">
          Tezbar · v{version}
        </p>
        <h1 id="release-notes-title" className="text-xl font-semibold">
          {afterUpdate ? 'Tezbar has been updated' : 'What’s new in Tezbar'}
        </h1>
        {notes?.publishedAt && (
          <p className="mt-1 text-xs text-ink-3">
            {new Date(notes.publishedAt).toLocaleDateString(undefined, {
              month: 'long',
              day: 'numeric',
              year: 'numeric',
            })}
          </p>
        )}
      </header>
      {afterUpdate && (
        <p className="px-6 pt-4 text-sm text-ink-3">Here’s what’s new in this version.</p>
      )}
      <div className="max-h-[55vh] overflow-y-auto px-6 py-5">
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
      <footer className="flex justify-end border-t border-white/10 px-6 py-4">
        <Button variant="primary" autoFocus onClick={onClose}>
          Got it
        </Button>
      </footer>
    </dialog>
  )
}

/** Only mounted in the launcher after its initial onboarding check. */
export function PostUpdateReleaseNotes({
  firstInstall,
}: {
  firstInstall: boolean
}): JSX.Element | null {
  const [notes, setNotes] = useState<ReleaseNotes>()
  useEffect(() => {
    const storage = window.localStorage
    try {
      if (!shouldShowPostUpdateNotes(storage, firstInstall)) return
    } catch {
      return
    }
    const controller = new AbortController()
    const timer = window.setTimeout(() => controller.abort(), 10_000)
    void fetchReleaseNotes(CURRENT_APP_VERSION, controller.signal)
      .then((release) => {
        if (!controller.signal.aborted && release.body.trim()) setNotes(release)
      })
      .catch(() => undefined)
    return () => {
      controller.abort()
      window.clearTimeout(timer)
    }
  }, [firstInstall])
  if (!notes || firstInstall) return null
  return (
    <ReleaseNotesDialog
      version={notes.version}
      initialNotes={notes}
      afterUpdate
      onClose={() => {
        try {
          window.localStorage.setItem(LAST_RELEASE_NOTES_VERSION_KEY, notes.version)
        } catch {
          /* Session dismissal still works. */
        }
        setNotes(undefined)
      }}
    />
  )
}
