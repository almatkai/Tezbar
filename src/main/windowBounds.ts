import type { BrowserWindow } from 'electron'

/** Launcher dimensions. Width is a hard constant. Height floats inside the
 *  allowed range; the renderer drives it via IPC. The OS cannot drag the
 *  window to resize (resizable: false in the BrowserWindow config). */
export const WINDOW_WIDTH = 760
/**
 * Upper bound when the launcher hosts a multi-turn chat conversation. We
 * keep the original 480px envelope for compact states (search results,
 * empty bar, single answer) but allow the window to grow taller when the
 * renderer reports a real chat thread. The renderer drives this via the
 * ResizeObserver → `setLauncherContentHeight` path, so the cap is the
 * absolute ceiling, not the default.
 */
export const WINDOW_MAX_HEIGHT = 640
export const WINDOW_MIN_HEIGHT = 120
export const WINDOW_TOP_FACTOR = 0.2

export function clampLauncherHeight(height: number): number {
  if (!Number.isFinite(height)) return WINDOW_MAX_HEIGHT
  return Math.min(Math.max(Math.round(height), WINDOW_MIN_HEIGHT), WINDOW_MAX_HEIGHT)
}

/** Programmatically set the launcher height from the renderer. Width is
 *  locked; only the height can change. Ignores no-op updates so we don't
 *  thrash the window on every ResizeObserver tick. */
export function setLauncherContentHeight(
  win: BrowserWindow,
  rawHeight: number,
  rawZoomFactor = 1
): void {
  const zoomFactor =
    Number.isFinite(rawZoomFactor) && rawZoomFactor > 0 ? Math.max(1, rawZoomFactor) : 1
  const maxHeight = Math.round(WINDOW_MAX_HEIGHT * zoomFactor)
  const height = Math.min(
    Math.max(Math.round(rawHeight), WINDOW_MIN_HEIGHT),
    maxHeight
  )
  win.setMaximumSize(WINDOW_WIDTH, maxHeight)
  const [curW, curH] = win.getContentSize()
  if (curW === WINDOW_WIDTH && curH === height) return
  win.setContentSize(WINDOW_WIDTH, height, false)
}
