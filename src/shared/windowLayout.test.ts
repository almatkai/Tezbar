import { describe, expect, it } from 'vitest'
import { launcherZoomAfterKey } from './windowLayout'

describe('launcher UI scale shortcuts', () => {
  it('zooms in/out and resets without changing native window dimensions', () => {
    expect(launcherZoomAfterKey(1, '+')).toBe(1.1)
    expect(launcherZoomAfterKey(1, '=')).toBe(1.1)
    expect(launcherZoomAfterKey(1, '-')).toBe(0.9)
    expect(launcherZoomAfterKey(1.75, '0')).toBe(1)
  })
  it('bounds scale and avoids floating point drift', () => {
    expect(launcherZoomAfterKey(2, '+')).toBe(2)
    expect(launcherZoomAfterKey(0.75, '-')).toBe(0.75)
    expect(launcherZoomAfterKey(1.1, '-')).toBe(1)
    expect(launcherZoomAfterKey(1.25, '+')).toBe(1.35)
  })
  it('does not consume unrelated keys', () => {
    expect(launcherZoomAfterKey(1, 'Escape')).toBeNull()
    expect(launcherZoomAfterKey(1, 'n')).toBeNull()
  })
})
