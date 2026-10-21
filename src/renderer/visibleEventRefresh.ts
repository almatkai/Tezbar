// Refresh on backend changes while visible; keep a slow fallback for missed events.
export function visibleEventRefresh(
  refresh: () => void,
  subscribe: (listener: () => void) => () => void
): () => void {
  let interval: number | null = null
  let pending: number | null = null
  let stopped = false

  const queue = (): void => {
    if (stopped || document.hidden) return
    if (pending !== null) window.clearTimeout(pending)
    pending = window.setTimeout(() => {
      pending = null
      if (!stopped && !document.hidden) refresh()
    }, 150)
  }
  const start = (): void => {
    if (stopped || document.hidden) return
    if (interval === null)
      interval = window.setInterval(() => {
        if (!document.hidden) refresh()
      }, 20_000)
    queue()
  }
  const onVisibility = (): void => {
    if (document.hidden) {
      if (interval !== null) window.clearInterval(interval)
      interval = null
      if (pending !== null) window.clearTimeout(pending)
      pending = null
    } else {
      start()
    }
  }

  const unsubscribe = subscribe(queue)
  const offWindowShown = window.tezbar.onWindowShown(start)
  document.addEventListener('visibilitychange', onVisibility)
  window.addEventListener('focus', start)
  start()

  return () => {
    stopped = true
    if (interval !== null) window.clearInterval(interval)
    if (pending !== null) window.clearTimeout(pending)
    unsubscribe()
    offWindowShown()
    document.removeEventListener('visibilitychange', onVisibility)
    window.removeEventListener('focus', start)
  }
}
