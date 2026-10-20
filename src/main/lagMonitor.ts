import { monitorEventLoopDelay } from 'node:perf_hooks'

/**
 * Event-loop stall reporting for the backend process.
 *
 * Every IPC reply shares one event loop, so a single synchronous operation
 * (an index batch, a large directory walk, a big SQLite statement) delays the
 * launcher, chat history and every other view at once. `slow IPC` warnings
 * only show the *symptom*: a channel that finished late. This monitor reports
 * the blocking window itself, which is what has to be deleted.
 */

/** Below this the stall is ordinary jitter, not a user-visible freeze. */
export const STALL_THRESHOLD_MS = 750

/** Convert a histogram max (nanoseconds) into a reported stall in ms. */
export function lagStallMs(maxNs: number, thresholdMs = STALL_THRESHOLD_MS): number | null {
  if (!Number.isFinite(maxNs) || maxNs <= 0) return null
  const ms = maxNs / 1e6
  if (ms < thresholdMs) return null
  return Math.round(ms)
}

export function startLagMonitor(
  onStall: (stallMs: number) => void,
  options: { thresholdMs?: number; intervalMs?: number } = {}
): () => void {
  const thresholdMs = options.thresholdMs ?? STALL_THRESHOLD_MS
  const intervalMs = options.intervalMs ?? 5_000

  let histogram: ReturnType<typeof monitorEventLoopDelay> | null = null
  try {
    if (typeof monitorEventLoopDelay === 'function') {
      const candidate = monitorEventLoopDelay({ resolution: 100 })
      candidate.enable()
      histogram = candidate
    }
  } catch {
    // Bun (and some other non-Node runtimes) does not implement monitorEventLoopDelay
    histogram = null
  }

  if (histogram) {
    const timer = setInterval(() => {
      const stall = lagStallMs(histogram!.max, thresholdMs)
      histogram!.reset()
      if (stall !== null) onStall(stall)
    }, intervalMs)
    timer.unref?.()

    return () => {
      clearInterval(timer)
      histogram?.disable()
    }
  }

  // Fallback for runtimes without monitorEventLoopDelay (e.g. Bun):
  // measure scheduler delay using a lightweight heartbeat.
  const pollMs = Math.min(250, Math.max(50, Math.floor(intervalMs / 10)))
  let lastTick = Date.now()
  const timer = setInterval(() => {
    const now = Date.now()
    const delay = now - lastTick - pollMs
    lastTick = now
    if (delay >= thresholdMs) {
      onStall(Math.round(delay))
    }
  }, pollMs)
  timer.unref?.()

  return () => {
    clearInterval(timer)
  }
}
