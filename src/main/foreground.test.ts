import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  FOREGROUND_HOLD_MS,
  backgroundHoldRemainingMs,
  countsAsForegroundActivity,
  foregroundActive,
  foregroundRemainingMs,
  noteForegroundActivity,
  resetForegroundGate,
  waitForForegroundIdle,
} from './foreground'

afterEach(() => {
  resetForegroundGate()
  vi.useRealTimers()
})

describe('foreground gate', () => {
  it('holds background work for a short window after a user request', () => {
    noteForegroundActivity(undefined, 1_000)

    expect(foregroundActive(1_000)).toBe(true)
    expect(foregroundRemainingMs(1_000)).toBe(FOREGROUND_HOLD_MS)
    expect(foregroundActive(1_000 + FOREGROUND_HOLD_MS - 1)).toBe(true)
    expect(foregroundActive(1_000 + FOREGROUND_HOLD_MS)).toBe(false)
  })

  it('never shortens a window that is already open', () => {
    noteForegroundActivity(2_000, 1_000)
    noteForegroundActivity(200, 1_100)

    expect(foregroundRemainingMs(1_100)).toBe(1_900)
  })

  it('releases background work only after the last request settles', async () => {
    vi.useFakeTimers()
    noteForegroundActivity(300, Date.now())

    let released = false
    const waiting = waitForForegroundIdle().then(() => {
      released = true
    })

    await vi.advanceTimersByTimeAsync(250)
    expect(released).toBe(false)
    noteForegroundActivity(300, Date.now())
    await vi.advanceTimersByTimeAsync(250)
    expect(released).toBe(false)
    await vi.advanceTimersByTimeAsync(200)
    await waiting
    expect(released).toBe(true)
  })

  it('stops waiting when the caller aborts', async () => {
    noteForegroundActivity(5_000, Date.now())
    const controller = new AbortController()
    const waiting = waitForForegroundIdle(controller.signal)

    controller.abort()
    await expect(waiting).resolves.toBeUndefined()
  })

  it('lets a caller keep its own longer pause while nothing is interactive', () => {
    noteForegroundActivity(200, 1_000)
    expect(backgroundHoldRemainingMs(3_000, 1_000)).toBe(2_000)
    expect(backgroundHoldRemainingMs(0, 1_000)).toBe(200)
    expect(backgroundHoldRemainingMs(0, 5_000)).toBe(0)
  })

  it('treats status polling as background so indexing is not paused forever', () => {
    expect(countsAsForegroundActivity('chat:list')).toBe(true)
    expect(countsAsForegroundActivity('chat:get')).toBe(true)
    expect(countsAsForegroundActivity('agent:run')).toBe(true)
    expect(countsAsForegroundActivity('query')).toBe(true)

    expect(countsAsForegroundActivity('background-tasks:list')).toBe(false)
    expect(countsAsForegroundActivity('knowledge:snapshot')).toBe(false)
    expect(countsAsForegroundActivity('terminal:list')).toBe(false)
    expect(countsAsForegroundActivity('extension:refresh-session')).toBe(false)
  })
})
