import { describe, expect, it } from 'vitest'
import {
  readAutoUpdatePreference,
  readLastUpdateCheck,
  recordUpdateCheck,
  shouldAutoCheckForUpdates,
  UPDATE_CHECK_INTERVAL_MS,
  writeAutoUpdatePreference,
} from './updater'

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
