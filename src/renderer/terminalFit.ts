import type { Terminal } from '@xterm/xterm'
import type { FitAddon } from '@xterm/addon-fit'

/** Keep fitting tied to the host, rather than the terminal's own canvas size. */
export function observeTerminalFit(
  host: HTMLElement,
  terminal: Terminal,
  fitAddon: FitAddon,
  fit: () => void,
): () => void {
  let disposed = false
  let frame = 0
  const scheduleFit = (): void => {
    cancelAnimationFrame(frame)
    frame = requestAnimationFrame(() => {
      if (!disposed) fit()
    })
  }
  const observer = new ResizeObserver(scheduleFit)
  observer.observe(host)
  // The first host resize can arrive before xterm has measured its cells.
  // Font changes can also change the grid without changing the host's size.
  // Only refit a mismatched grid so rendering output cannot create a fit loop.
  const renderSubscription = terminal.onRender(() => {
    const dimensions = fitAddon.proposeDimensions()
    if (
      dimensions &&
      Number.isFinite(dimensions.cols) &&
      Number.isFinite(dimensions.rows) &&
      (dimensions.cols !== terminal.cols || dimensions.rows !== terminal.rows)
    ) {
      scheduleFit()
    }
  })
  scheduleFit()
  return () => {
    disposed = true
    cancelAnimationFrame(frame)
    observer.disconnect()
    renderSubscription.dispose()
  }
}
