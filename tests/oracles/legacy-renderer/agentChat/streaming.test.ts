import { describe, expect, it, vi } from 'vitest'
import { createFrameBatcher, isNearScrollBottom } from './streaming'

function createAnimationFrameHarness(): {
  requestFrame: (callback: FrameRequestCallback) => number
  cancelFrame: (id: number) => void
  runNextFrame: (timestamp: number) => void
  queuedFrames: () => number
} {
  let nextId = 1
  const callbacks = new Map<number, FrameRequestCallback>()

  return {
    requestFrame(callback) {
      const id = nextId++
      callbacks.set(id, callback)
      return id
    },
    cancelFrame(id) {
      callbacks.delete(id)
    },
    runNextFrame(timestamp) {
      const entry = callbacks.entries().next().value as [number, FrameRequestCallback] | undefined
      if (!entry) throw new Error('No animation frame is queued')
      callbacks.delete(entry[0])
      entry[1](timestamp)
    },
    queuedFrames() {
      return callbacks.size
    },
  }
}

describe('createFrameBatcher', () => {
  it('coalesces bursty agent deltas and caps commits to the configured frame rate', () => {
    const frames = createAnimationFrameHarness()
    const onFlush = vi.fn()
    const batcher = createFrameBatcher({
      ...frames,
      minIntervalMs: 32,
      onFlush,
    })

    for (let index = 0; index < 100; index += 1) batcher.schedule()

    expect(frames.queuedFrames()).toBe(1)
    frames.runNextFrame(100)
    expect(onFlush).toHaveBeenCalledTimes(1)

    for (let index = 0; index < 20; index += 1) batcher.schedule()
    frames.runNextFrame(116)
    expect(onFlush).toHaveBeenCalledTimes(1)
    expect(frames.queuedFrames()).toBe(1)

    frames.runNextFrame(132)
    expect(onFlush).toHaveBeenCalledTimes(2)
    expect(frames.queuedFrames()).toBe(0)
  })

  it('can flush the latest state immediately and cancel stale scheduled work', () => {
    const frames = createAnimationFrameHarness()
    const onFlush = vi.fn()
    const batcher = createFrameBatcher({
      ...frames,
      minIntervalMs: 32,
      onFlush,
    })

    batcher.schedule()
    batcher.flush()

    expect(onFlush).toHaveBeenCalledTimes(1)
    expect(frames.queuedFrames()).toBe(0)

    batcher.schedule()
    batcher.cancel()
    expect(frames.queuedFrames()).toBe(0)
  })
})

describe('isNearScrollBottom', () => {
  it('follows output only while the reader remains near the latest content', () => {
    expect(isNearScrollBottom({ scrollTop: 500, scrollHeight: 1_000, clientHeight: 460 })).toBe(
      true
    )
    expect(isNearScrollBottom({ scrollTop: 300, scrollHeight: 1_000, clientHeight: 460 })).toBe(
      false
    )
  })
})
