// src/shared/updater.ts
//
// Types and helpers for the app update tracker.

export type AppUpdateStatus =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'upToDate'; version: string }
  | { kind: 'available'; version: string; notes: string; releaseUrl: string }
  | { kind: 'downloading'; version: string; downloaded: number; total: number | null }
  | { kind: 'ready'; version: string }
  | { kind: 'error'; message: string }

export const RELEASES_PAGE_URL = 'https://github.com/almatkai/Tezbar/releases'

export const LAST_UPDATE_CHECK_KEY = 'tezbar:last-update-check'
export const AUTO_UPDATE_PREFERENCE_KEY = 'tezbar:auto-update-enabled'

/** Check every 4 hours, matching desktop update standards. */
export const UPDATE_CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000

export function shouldAutoCheckForUpdates(
  lastCheckedAt: number | null,
  now: number = Date.now()
): boolean {
  if (lastCheckedAt === null || !Number.isFinite(lastCheckedAt)) return true
  return now - lastCheckedAt > UPDATE_CHECK_INTERVAL_MS
}

export function readLastUpdateCheck(storage: Pick<Storage, 'getItem'>): number | null {
  const raw = storage.getItem(LAST_UPDATE_CHECK_KEY)
  if (!raw) return null
  const parsed = Number(raw)
  return Number.isFinite(parsed) ? parsed : null
}

export function recordUpdateCheck(
  storage: Pick<Storage, 'setItem'>,
  now: number = Date.now()
): void {
  storage.setItem(LAST_UPDATE_CHECK_KEY, String(now))
}

export function readAutoUpdatePreference(storage: Pick<Storage, 'getItem'>): boolean {
  return storage.getItem(AUTO_UPDATE_PREFERENCE_KEY) === 'true'
}

export function writeAutoUpdatePreference(
  storage: Pick<Storage, 'setItem'>,
  enabled: boolean
): void {
  storage.setItem(AUTO_UPDATE_PREFERENCE_KEY, String(enabled))
}
