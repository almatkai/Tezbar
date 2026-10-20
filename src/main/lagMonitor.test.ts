import { describe, expect, it, vi } from 'vitest'
import { STALL_THRESHOLD_MS, lagStallMs, startLagMonitor } from './lagMonitor'

describe('backend event-loop stall reporting', () => {
  it('ignores ordinary scheduling jitter', () => {
    expect(lagStallMs(2_000_000)).toBeNull()
    expect(lagStallMs((STALL_THRESHOLD_MS - 1) * 1e6)).toBeNull()
  })

  it('reports a blocking window in milliseconds', () => {
    expect(lagStallMs(6_600 * 1e6)).toBe(6_600)
    expect(lagStallMs(STALL_THRESHOLD_MS * 1e6)).toBe(STALL_THRESHOLD_MS)
  })

  it('ignores an empty or malformed histogram', () => {
    expect(lagStallMs(0)).toBeNull()
    expect(lagStallMs(Number.NaN)).toBeNull()
    expect(lagStallMs(-5)).toBeNull()
  })

  it('starts and stops cleanly without throwing in any runtime environment', () => {
    const onStall = vi.fn()
    const stop = startLagMonitor(onStall, { intervalMs: 1_000 })
    expect(typeof stop).toBe('function')
    expect(() => stop()).not.toThrow()
  })
})
