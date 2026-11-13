const DEFAULT_SCROLL_BOTTOM_THRESHOLD_PX = 64

type FrameBatcherOptions = {
  minIntervalMs: number
  onFlush: () => void
  requestFrame: (callback: FrameRequestCallback) => number
  cancelFrame: (id: number) => void
  now?: () => number
}

export type FrameBatcher = {
  schedule: () => void
  flush: () => void
  cancel: () => void
}

/**
 * Coalesces a burst of IPC stream events into a paint-aligned update. The
 * interval cap deliberately keeps text generation below the display refresh
 * rate so markdown/layout work cannot consume every available frame.
 */
export function createFrameBatcher({
  minIntervalMs,
  onFlush,
  requestFrame,
  cancelFrame,
  now = () => performance.now(),
}: FrameBatcherOptions): FrameBatcher {
  let scheduledFrame: number | null = null
  let pending = false
  let lastFlushAt = Number.NEGATIVE_INFINITY

  const onFrame = (timestamp: number): void => {
    scheduledFrame = null
    if (!pending) return

    if (timestamp - lastFlushAt < minIntervalMs) {
      scheduledFrame = requestFrame(onFrame)
      return
    }

    pending = false
    lastFlushAt = timestamp
    onFlush()
  }

  return {
    schedule() {
      pending = true
      if (scheduledFrame === null) {
        scheduledFrame = requestFrame(onFrame)
      }
    },
    flush() {
      if (scheduledFrame !== null) {
        cancelFrame(scheduledFrame)
        scheduledFrame = null
      }
      if (!pending) return
      pending = false
      lastFlushAt = now()
      onFlush()
    },
    cancel() {
      pending = false
      if (scheduledFrame !== null) {
        cancelFrame(scheduledFrame)
        scheduledFrame = null
      }
    },
  }
}

export function isNearScrollBottom(
  {
    scrollTop,
    scrollHeight,
    clientHeight,
  }: Pick<HTMLElement, 'scrollTop' | 'scrollHeight' | 'clientHeight'>,
  thresholdPx = DEFAULT_SCROLL_BOTTOM_THRESHOLD_PX
): boolean {
  return scrollHeight - clientHeight - scrollTop <= thresholdPx
}
