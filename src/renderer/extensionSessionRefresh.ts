import type { ExtensionSessionInvalidation } from '../shared/extensionRuntime'

/** One refresh at a time; invalidations during a request coalesce into one more.
 * Undefined policy preserves V1 polling, null never creates a polling timer. */
export function startSessionRefresh(options: {
  sessionId: string
  generation?: string
  refreshMs?: number | null
  refresh: () => Promise<void>
  subscribe?: (listener: (event: ExtensionSessionInvalidation) => void) => () => void
  subscribeVisibility?: (listener: (visible: boolean) => void) => () => void
  visibility?: Document
}): () => void {
  const interval = options.refreshMs === undefined ? 500 : options.refreshMs
  const visibility = options.visibility ?? document
  let windowVisible = true
  let stopped = false
  let running = false
  let dirty = false
  let revision = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  const visible = (): boolean => windowVisible && visibility.visibilityState !== 'hidden'
  const schedule = (): void => {
    if (!stopped && visible() && interval !== null) {
      timer = setTimeout(() => void refresh(), Math.max(500, interval))
    }
  }
  const refresh = async (): Promise<void> => {
    if (stopped || !visible()) {
      dirty = true
      return
    }
    if (running) {
      dirty = true
      return
    }
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
    running = true
    dirty = false
    try {
      await options.refresh()
    } finally {
      running = false
      if (!stopped && dirty && visible()) void refresh()
      else schedule()
    }
  }
  const unsubscribe = options.subscribe?.((event) => {
    if (
      event.sessionId !== options.sessionId ||
      event.generation !== options.generation ||
      event.revision <= revision
    )
      return
    revision = event.revision
    void refresh()
  })
  const changed = (): void => {
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
    if (visible()) {
      if (dirty || interval !== null) void refresh()
    }
  }
  const offVisibility = options.subscribeVisibility?.((value) => {
    windowVisible = value
    changed()
  })
  visibility.addEventListener('visibilitychange', changed)
  schedule()
  return () => {
    stopped = true
    if (timer !== undefined) clearTimeout(timer)
    unsubscribe?.()
    offVisibility?.()
    visibility.removeEventListener('visibilitychange', changed)
  }
}
