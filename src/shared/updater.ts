// src/shared/updater.ts
//
// Types and helpers for the app update tracker.

export type AppUpdateStatus =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'upToDate'; version: string }
  | { kind: 'available'; version: string; notes: string; releaseUrl: string }
  | {
      kind: 'manualAvailable'
      version: string
      notes: string
      releaseUrl: string
      installerUrl: string
    }
  | { kind: 'downloading'; version: string; downloaded: number; total: number | null }
  | { kind: 'ready'; version: string }
  | { kind: 'error'; message: string }

export const RELEASES_PAGE_URL = 'https://github.com/almatkai/Tezbar/releases'
export type ReleaseChannel = 'stable' | 'beta' | 'alpha'
export type UpdateChannelInfo = { channel: ReleaseChannel; version: string }

export function channelDescription(info: UpdateChannelInfo): string {
  if (info.channel === 'alpha')
    return 'Experimental Alpha releases, plus Beta and Stable updates. Early releases use your current settings and data; switching channels never downgrades the app.'
  if (info.channel === 'beta')
    return info.version.includes('-alpha.')
      ? 'You’ll receive the next Beta or Stable release newer than this Alpha build. Your settings and data stay in place.'
      : 'Beta and Stable updates. Early releases use your current settings and data; Alpha releases are excluded.'
  return /-(alpha|beta)\./.test(info.version)
    ? 'You’ll receive the next Stable release newer than this prerelease build. Your settings and data stay in place.'
    : 'Recommended for everyday use. Only Stable updates are offered.'
}

export const LAST_UPDATE_CHECK_KEY = 'tezbar:last-update-check'
export const AUTO_UPDATE_PREFERENCE_KEY = 'tezbar:auto-update-enabled'

/** Check every 12 hours, matching update check validity standard. */
export const UPDATE_CHECK_INTERVAL_MS = 12 * 60 * 60 * 1000

/** 12 hours validity period for up-to-date checks. */
export const UP_TO_DATE_VALIDITY_MS = 12 * 60 * 60 * 1000

export function isUpToDateFresh(lastCheckedAt: number | null, now: number = Date.now()): boolean {
  if (lastCheckedAt === null || !Number.isFinite(lastCheckedAt)) return false
  const elapsed = now - lastCheckedAt
  return elapsed >= 0 && elapsed < UP_TO_DATE_VALIDITY_MS
}

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

export function computeEffectiveUpdateStatus(
  status: AppUpdateStatus,
  lastCheckedAt: number | null,
  now: number = Date.now()
): AppUpdateStatus {
  if (status.kind !== 'upToDate') {
    return status
  }
  return isUpToDateFresh(lastCheckedAt, now) ? status : { kind: 'idle' }
}

export function isUrgentUpdateNotification(status: AppUpdateStatus): boolean {
  return (
    status.kind === 'available' ||
    status.kind === 'manualAvailable' ||
    status.kind === 'ready' ||
    status.kind === 'downloading'
  )
}

export function matchesUpdateSearchQuery(
  rawQuery: string,
  status: AppUpdateStatus,
  isCheckedInCurrentWindow: boolean = false
): boolean {
  const q = rawQuery.trim().toLowerCase()
  if (!q) {
    return (
      isUrgentUpdateNotification(status) || (status.kind === 'checking' && isCheckedInCurrentWindow)
    )
  }
  return (
    'update'.includes(q) ||
    'tezbar'.includes(q) ||
    'upgrade'.includes(q) ||
    'обновление'.includes(q) ||
    'обнова'.includes(q) ||
    'версия'.includes(q) ||
    'check for updates'.includes(q)
  )
}

export function shouldPromoteUpdateCommand(
  status: AppUpdateStatus,
  isCheckedInCurrentWindow: boolean = false
): boolean {
  return (
    isUrgentUpdateNotification(status) ||
    (status.kind === 'checking' && isCheckedInCurrentWindow) ||
    (status.kind === 'upToDate' && isCheckedInCurrentWindow)
  )
}

export function getUpdateCommandScore(
  status: AppUpdateStatus,
  baseScore?: number,
  isDowngraded: boolean = false
): number {
  if (status.kind === 'upToDate' && isDowngraded) {
    return Math.min(baseScore ?? 50, 100)
  }
  if (status.kind === 'error') {
    return baseScore ?? 100
  }
  return baseScore ?? 1_000_000
}
