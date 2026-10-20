import type { ChatSessionSummary } from '../../shared/chat'

/**
 * The conversation list outlives a single launcher mount.
 *
 * Switching surfaces remounts the launcher (the surface wrapper is keyed), so
 * coming back from an open chat used to start again from an empty list and wait
 * for the backend. Painting the last known rows immediately keeps Back instant
 * even when the backend is busy indexing.
 */
let cached: ChatSessionSummary[] | null = null

export function readCachedChatHistory(): ChatSessionSummary[] | null {
  return cached
}

export function writeCachedChatHistory(rows: ChatSessionSummary[]): void {
  cached = rows
}

/** Test hook. */
export function clearCachedChatHistory(): void {
  cached = null
}
