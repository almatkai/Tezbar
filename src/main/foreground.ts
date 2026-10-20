/**
 * Foreground gate.
 *
 * Every IPC reply the UI waits for shares this backend's event loop with
 * background indexing. When a user-facing request arrives we open a short
 * "foreground" window; background loops call {@link waitForForegroundIdle}
 * between batches and step aside, so opening chat history or the launcher
 * never queues behind an indexing batch.
 *
 * Status polling is deliberately excluded: a poll every second would otherwise
 * hold the gate open forever and stall indexing completely.
 */

/** How long background work stays out of the way after the last request. */
export const FOREGROUND_HOLD_MS = 1_200

/** Granularity of the wait loop. Short enough to react, cheap enough to idle. */
const POLL_INTERVAL_MS = 25

const BACKGROUND_CHANNELS = new Set([
  // Polled while a view is open, not a user action on its own.
  'background-tasks:list',
  'terminal:list',
  'extension:refresh-session',
])

const BACKGROUND_PREFIXES = ['knowledge:', 'update:', 'updates:', 'updater:']

let holdUntil = 0

/** Channels that represent something the user is waiting for. */
export function countsAsForegroundActivity(channel: string): boolean {
  if (BACKGROUND_CHANNELS.has(channel)) return false
  return !BACKGROUND_PREFIXES.some((prefix) => channel.startsWith(prefix))
}

/** Keep background work out of the way for another `durationMs`. */
export function noteForegroundActivity(
  durationMs: number = FOREGROUND_HOLD_MS,
  now: number = Date.now()
): void {
  holdUntil = Math.max(holdUntil, now + Math.max(0, durationMs))
}

export function foregroundRemainingMs(now: number = Date.now()): number {
  return Math.max(0, holdUntil - now)
}

export function foregroundActive(now: number = Date.now()): boolean {
  return foregroundRemainingMs(now) > 0
}

/** How long background knowledge work should keep waiting: whichever is later,
 *  the caller's own pause deadline or the current foreground window. */
export function backgroundHoldRemainingMs(localUntil = 0, now: number = Date.now()): number {
  return Math.max(0, localUntil - now, foregroundRemainingMs(now))
}

/** Resolve once no user-facing request is waiting. Resolves immediately when
 *  `signal` aborts so a paused indexer can shut down promptly. */
export async function waitForForegroundIdle(signal?: AbortSignal): Promise<void> {
  while (!signal?.aborted && foregroundActive()) {
    const remaining = foregroundRemainingMs()
    await new Promise<void>((resolve) =>
      setTimeout(resolve, Math.min(POLL_INTERVAL_MS, Math.max(1, remaining)))
    )
  }
}

/** Test hook: drop any open window. */
export function resetForegroundGate(): void {
  holdUntil = 0
}
