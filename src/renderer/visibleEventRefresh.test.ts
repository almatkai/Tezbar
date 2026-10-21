import { afterEach, describe, expect, it, vi } from 'vitest'
import { visibleEventRefresh } from './visibleEventRefresh'

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('visibleEventRefresh', () => {
  it('coalesces events, pauses while hidden and refreshes on show', () => {
    vi.useFakeTimers()
    let hidden = false
    const documentEvents = new EventTarget()
    Object.defineProperty(documentEvents, 'hidden', { get: () => hidden })
    const windowEvents = new EventTarget()
    Object.assign(windowEvents, {
      setTimeout,
      clearTimeout,
      setInterval,
      clearInterval,
      tezbar: {
        onWindowShown: (callback: () => void) => {
          windowEvents.addEventListener('shown', callback)
          return () => windowEvents.removeEventListener('shown', callback)
        },
      },
    })
    vi.stubGlobal('document', documentEvents)
    vi.stubGlobal('window', windowEvents)
    const refresh = vi.fn()
    let changed = () => {}
    const unsubscribe = vi.fn()
    const stop = visibleEventRefresh(refresh, (listener) => {
      changed = listener
      return unsubscribe
    })

    // The initial load is immediate rather than debounced.
    expect(refresh).toHaveBeenCalledTimes(1)
    changed()
    changed()
    vi.advanceTimersByTime(149)
    expect(refresh).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(1)
    expect(refresh).toHaveBeenCalledTimes(2)
    hidden = true
    documentEvents.dispatchEvent(new Event('visibilitychange'))
    changed()
    vi.advanceTimersByTime(30_000)
    expect(refresh).toHaveBeenCalledTimes(2)
    hidden = false
    documentEvents.dispatchEvent(new Event('visibilitychange'))
    expect(refresh).toHaveBeenCalledTimes(3)
    windowEvents.dispatchEvent(new Event('shown'))
    expect(refresh).toHaveBeenCalledTimes(4)
    vi.advanceTimersByTime(20_000)
    expect(refresh).toHaveBeenCalledTimes(5)
    stop()
    vi.advanceTimersByTime(30_000)
    expect(refresh).toHaveBeenCalledTimes(5)
    expect(unsubscribe).toHaveBeenCalledOnce()
  })
})
