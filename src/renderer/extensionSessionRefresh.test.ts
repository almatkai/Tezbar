// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { startSessionRefresh } from './extensionSessionRefresh'
import type { ExtensionSessionInvalidation } from '../shared/extensionRuntime'

afterEach(() => vi.useRealTimers())
describe('process session refresh', () => {
  it('static views have zero polling and accept only newer owned invalidations', async () => {
    vi.useFakeTimers()
    let event!: (e: ExtensionSessionInvalidation) => void
    const refresh = vi.fn(async () => {})
    const unsub = vi.fn()
    const stop = startSessionRefresh({
      sessionId: 's',
      generation: 'g',
      refreshMs: null,
      refresh,
      subscribe: (listener) => {
        event = listener
        return unsub
      },
    })
    await vi.advanceTimersByTimeAsync(60_000)
    expect(refresh).not.toHaveBeenCalled()
    event({ sessionId: 'other', generation: 'g', revision: 1 })
    event({ sessionId: 's', generation: 'old', revision: 1 })
    expect(refresh).not.toHaveBeenCalled()
    event({ sessionId: 's', generation: 'g', revision: 2 })
    await Promise.resolve()
    event({ sessionId: 's', generation: 'g', revision: 2 })
    expect(refresh).toHaveBeenCalledTimes(1)
    stop()
    expect(unsub).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(60_000)
    expect(refresh).toHaveBeenCalledTimes(1)
  })
  it('preserves V1 polling, coalesces events and stops on disposal', async () => {
    vi.useFakeTimers()
    let finish!: () => void
    let event!: (e: ExtensionSessionInvalidation) => void
    const refresh = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve
        })
    )
    const stop = startSessionRefresh({
      sessionId: 's',
      generation: 'g',
      refresh,
      subscribe: (listener) => {
        event = listener
        return () => {}
      },
    })
    await vi.advanceTimersByTimeAsync(500)
    expect(refresh).toHaveBeenCalledTimes(1)
    event({ sessionId: 's', generation: 'g', revision: 1 })
    event({ sessionId: 's', generation: 'g', revision: 2 })
    expect(refresh).toHaveBeenCalledTimes(1)
    finish()
    await Promise.resolve()
    expect(refresh).toHaveBeenCalledTimes(2)
    stop()
    finish()
    await vi.advanceTimersByTimeAsync(5000)
    expect(refresh).toHaveBeenCalledTimes(2)
  })
})

it('defers hidden-window invalidation and resumes once visible', async () => {
  vi.useFakeTimers()
  let event!: (e: ExtensionSessionInvalidation) => void
  let visibility!: (visible: boolean) => void
  const refresh = vi.fn(async () => {})
  const stop = startSessionRefresh({
    sessionId: 's',
    generation: 'g',
    refreshMs: null,
    refresh,
    subscribe: (listener) => {
      event = listener
      return () => {}
    },
    subscribeVisibility: (listener) => {
      visibility = listener
      return () => {}
    },
  })
  visibility(false)
  event({ sessionId: 's', generation: 'g', revision: 1 })
  event({ sessionId: 's', generation: 'g', revision: 2 })
  await vi.advanceTimersByTimeAsync(60_000)
  expect(refresh).not.toHaveBeenCalled()
  visibility(true)
  await Promise.resolve()
  expect(refresh).toHaveBeenCalledTimes(1)
  stop()
})
