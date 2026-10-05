import { describe, expect, it } from 'vitest'
import {
  computeEffectiveUpdateStatus,
  getUpdateCommandScore,
  isUpToDateFresh,
  isUrgentUpdateNotification,
  matchesUpdateSearchQuery,
  readAutoUpdatePreference,
  readLastUpdateCheck,
  recordUpdateCheck,
  shouldAutoCheckForUpdates,
  shouldPromoteUpdateCommand,
  UP_TO_DATE_VALIDITY_MS,
  UPDATE_CHECK_INTERVAL_MS,
  writeAutoUpdatePreference,
} from './updater'

describe('isUpToDateFresh', () => {
  it('returns false when never checked', () => {
    expect(isUpToDateFresh(null, 1_000_000)).toBe(false)
  })

  it('returns false when timestamp is NaN', () => {
    expect(isUpToDateFresh(Number.NaN, 1_000_000)).toBe(false)
  })

  it('returns true when checked within 12 hours', () => {
    const now = 100_000_000_000
    expect(isUpToDateFresh(now - 1_000, now)).toBe(true)
    expect(isUpToDateFresh(now - UP_TO_DATE_VALIDITY_MS + 1000, now)).toBe(true)
  })

  it('returns false when 12 hours have elapsed', () => {
    const now = 100_000_000_000
    expect(isUpToDateFresh(now - UP_TO_DATE_VALIDITY_MS, now)).toBe(false)
    expect(isUpToDateFresh(now - UP_TO_DATE_VALIDITY_MS - 1, now)).toBe(false)
  })

  it('returns false when timestamp is in the future', () => {
    const now = 100_000_000_000
    expect(isUpToDateFresh(now + 5_000, now)).toBe(false)
  })
})

describe('shouldAutoCheckForUpdates', () => {
  it('checks when never checked before', () => {
    expect(shouldAutoCheckForUpdates(null, 1_000_000)).toBe(true)
  })

  it('checks when the previous timestamp is invalid', () => {
    expect(shouldAutoCheckForUpdates(Number.NaN, 1_000_000)).toBe(true)
  })

  it('skips the check when the interval has not elapsed', () => {
    const now = 10_000_000
    expect(shouldAutoCheckForUpdates(now - 1_000, now)).toBe(false)
    expect(shouldAutoCheckForUpdates(now - UPDATE_CHECK_INTERVAL_MS + 1, now)).toBe(false)
  })

  it('checks again once the interval has elapsed', () => {
    const now = 100_000_000_000
    expect(shouldAutoCheckForUpdates(now - UPDATE_CHECK_INTERVAL_MS - 1, now)).toBe(true)
  })
})

describe('last-check storage', () => {
  const makeStorage = (initial?: Record<string, string>) => {
    const map = new Map<string, string>(Object.entries(initial ?? {}))
    return {
      getItem: (key: string) => map.get(key) ?? null,
      setItem: (key: string, value: string) => void map.set(key, value),
    }
  }

  it('round-trips the timestamp', () => {
    const storage = makeStorage()
    recordUpdateCheck(storage, 42)
    expect(readLastUpdateCheck(storage)).toBe(42)
  })

  it('returns null when unset or corrupt', () => {
    expect(readLastUpdateCheck(makeStorage())).toBeNull()
    expect(readLastUpdateCheck(makeStorage({ 'tezbar:last-update-check': 'not-a-number' }))).toBeNull()
  })

  it('reads and writes auto-update preference', () => {
    const storage = makeStorage()
    expect(readAutoUpdatePreference(storage)).toBe(false)
    writeAutoUpdatePreference(storage, true)
    expect(readAutoUpdatePreference(storage)).toBe(true)
    writeAutoUpdatePreference(storage, false)
    expect(readAutoUpdatePreference(storage)).toBe(false)
  })
})

describe('computeEffectiveUpdateStatus', () => {
  it('preserves non-upToDate statuses unchanged', () => {
    expect(computeEffectiveUpdateStatus({ kind: 'idle' }, null)).toEqual({ kind: 'idle' })
    expect(
      computeEffectiveUpdateStatus(
        { kind: 'available', version: '1.0.0', notes: '', releaseUrl: '' },
        null
      )
    ).toEqual({ kind: 'available', version: '1.0.0', notes: '', releaseUrl: '' })
    expect(computeEffectiveUpdateStatus({ kind: 'ready', version: '1.0.0' }, null)).toEqual({
      kind: 'ready',
      version: '1.0.0',
    })
    expect(computeEffectiveUpdateStatus({ kind: 'error', message: 'err' }, null)).toEqual({
      kind: 'error',
      message: 'err',
    })
  })

  it('keeps upToDate status when checked within 12 hours', () => {
    const now = 100_000_000_000
    const status = { kind: 'upToDate' as const, version: '0.2.0' }
    expect(computeEffectiveUpdateStatus(status, now - 3600_000, now)).toEqual(status)
  })

  it('reverts upToDate status to idle after 12 hours have elapsed', () => {
    const now = 100_000_000_000
    const status = { kind: 'upToDate' as const, version: '0.2.0' }
    expect(computeEffectiveUpdateStatus(status, now - UP_TO_DATE_VALIDITY_MS - 1, now)).toEqual({
      kind: 'idle',
    })
    expect(computeEffectiveUpdateStatus(status, null, now)).toEqual({ kind: 'idle' })
  })
})

describe('isUrgentUpdateNotification', () => {
  it('returns true only for available, ready, and downloading states', () => {
    expect(
      isUrgentUpdateNotification({
        kind: 'available',
        version: '1.0',
        notes: '',
        releaseUrl: '',
      })
    ).toBe(true)
    expect(isUrgentUpdateNotification({ kind: 'ready', version: '1.0' })).toBe(true)
    expect(
      isUrgentUpdateNotification({
        kind: 'downloading',
        version: '1.0',
        downloaded: 10,
        total: 100,
      })
    ).toBe(true)
    expect(isUrgentUpdateNotification({ kind: 'upToDate', version: '1.0' })).toBe(false)
    expect(isUrgentUpdateNotification({ kind: 'idle' })).toBe(false)
    expect(isUrgentUpdateNotification({ kind: 'checking' })).toBe(false)
    expect(isUrgentUpdateNotification({ kind: 'error', message: 'fail' })).toBe(false)
  })
})

describe('matchesUpdateSearchQuery', () => {
  it('does not match empty query for upToDate when not checked in current window', () => {
    expect(matchesUpdateSearchQuery('', { kind: 'upToDate', version: '1.0' }, false)).toBe(false)
    expect(matchesUpdateSearchQuery('   ', { kind: 'upToDate', version: '1.0' }, false)).toBe(false)
  })

  it('matches empty query for urgent update notifications', () => {
    expect(
      matchesUpdateSearchQuery(
        '',
        { kind: 'available', version: '1.0', notes: '', releaseUrl: '' },
        false
      )
    ).toBe(true)
    expect(matchesUpdateSearchQuery('', { kind: 'ready', version: '1.0' }, false)).toBe(true)
  })

  it('matches when user types update keywords for upToDate', () => {
    expect(matchesUpdateSearchQuery('update', { kind: 'upToDate', version: '1.0' })).toBe(true)
    expect(matchesUpdateSearchQuery('upd', { kind: 'upToDate', version: '1.0' })).toBe(true)
    expect(matchesUpdateSearchQuery('tezbar', { kind: 'upToDate', version: '1.0' })).toBe(true)
    expect(matchesUpdateSearchQuery('обновление', { kind: 'upToDate', version: '1.0' })).toBe(true)
  })

  it('does not match unrelated queries', () => {
    expect(matchesUpdateSearchQuery('safari', { kind: 'upToDate', version: '1.0' })).toBe(false)
    expect(matchesUpdateSearchQuery('terminal', { kind: 'upToDate', version: '1.0' })).toBe(false)
  })
})

describe('shouldPromoteUpdateCommand', () => {
  it('does not promote upToDate once the window has reopened (not in active check window)', () => {
    expect(shouldPromoteUpdateCommand({ kind: 'upToDate', version: '1.0' }, false)).toBe(false)
  })

  it('promotes upToDate immediately after checking in the current window', () => {
    expect(shouldPromoteUpdateCommand({ kind: 'upToDate', version: '1.0' }, true)).toBe(true)
  })

  it('promotes urgent update notifications regardless of window session', () => {
    expect(
      shouldPromoteUpdateCommand(
        { kind: 'available', version: '1.0', notes: '', releaseUrl: '' },
        false
      )
    ).toBe(true)
    expect(shouldPromoteUpdateCommand({ kind: 'ready', version: '1.0' }, false)).toBe(true)
  })

  it('does not promote idle status', () => {
    expect(shouldPromoteUpdateCommand({ kind: 'idle' }, false)).toBe(false)
  })
})

describe('getUpdateCommandScore', () => {
  it('downgrades score for upToDate when downgraded', () => {
    expect(getUpdateCommandScore({ kind: 'upToDate', version: '1.0' }, 200, true)).toBe(100)
    expect(getUpdateCommandScore({ kind: 'upToDate', version: '1.0' }, 30, true)).toBe(30)
    expect(getUpdateCommandScore({ kind: 'upToDate', version: '1.0' }, undefined, true)).toBe(50)
  })

  it('preserves top score when not downgraded or for urgent updates', () => {
    expect(getUpdateCommandScore({ kind: 'upToDate', version: '1.0' }, undefined, false)).toBe(
      1_000_000
    )
    expect(
      getUpdateCommandScore(
        { kind: 'available', version: '1.0', notes: '', releaseUrl: '' },
        undefined,
        false
      )
    ).toBe(1_000_000)
  })
})
