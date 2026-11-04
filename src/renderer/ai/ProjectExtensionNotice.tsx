import { useEffect, useState } from 'react'
import { extensionRequest as request } from './api'
const dismissed = new Set<string>()
export function ProjectExtensionNotice({
  cwd,
  onReview,
}: {
  cwd?: string
  onReview: () => void
}): JSX.Element | null {
  const [found, setFound] = useState(false)
  useEffect(() => {
    let cancelled = false
    setFound(false)
    if (cwd && !dismissed.has(cwd) && typeof window.tezbar.aiExtensions === 'function')
      void request<{ project?: unknown }>('list', cwd)
        .then((result) => {
          if (!cancelled) setFound(!!result.project)
        })
        .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [cwd])
  if (!cwd || !found || dismissed.has(cwd)) return null
  return (
    <div className="mb-2 flex items-center gap-2 text-[11px] text-ink-3">
      <span className="flex-1">This folder has AI extensions. Not running.</span>
      <button
        type="button"
        className="px-1 text-ink-2"
        onClick={() => {
          sessionStorage.setItem('tezbar-ai-extension-review', cwd)
          onReview()
        }}
      >
        Review
      </button>
      <button
        type="button"
        aria-label="Dismiss project extension notice"
        onClick={() => {
          dismissed.add(cwd)
          setFound(false)
        }}
      >
        ×
      </button>
    </div>
  )
}
