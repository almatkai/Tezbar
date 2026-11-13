const INTERACTIVE_TARGETS =
  '.no-drag, input, textarea, select, button, a[href], [role="button"], [role="menuitem"], [role="option"], [contenteditable="true"], .tezbar-popover, .agent-chat-shell > *, .tezbar-settings-window > *, .cleanmymac-sidebar'
const SEARCH_CONTROLS =
  '.no-drag, textarea, select, button, a[href], [role="button"], [role="menuitem"], [role="option"], [contenteditable="true"], .tezbar-popover'
const SEARCH_DRAG_THRESHOLD = 6

type WindowDragOptions = {
  start: () => Promise<unknown>
  end: () => Promise<unknown>
  isWindows: boolean
}

/** Search supports click-to-edit and drag-to-move; other chrome drags immediately. */
export function installWindowDrag({ start, end, isWindows }: WindowDragOptions): () => void {
  let dragActive = false
  let pendingSearchDrag: { x: number; y: number } | undefined

  const startDrag = (): void => {
    // On Windows the host watches the physical mouse button and ends the drag.
    if (!isWindows) dragActive = true
    void start()
  }

  const onMouseDown = (event: MouseEvent): void => {
    if (event.button !== 0) return
    const target = event.target
    if (!(target instanceof Element) || !target.closest('.drag-region')) return

    if (target.closest('[data-window-search-drag]')) {
      if (target.closest(SEARCH_CONTROLS)) return
      if (target.closest('input') && !target.matches('#command-input')) return
      // Preserve modified selection and double/triple-click word selection.
      if (event.shiftKey || event.metaKey || event.ctrlKey || event.altKey || event.detail > 1)
        return
      pendingSearchDrag = { x: event.screenX, y: event.screenY }
      return
    }

    if (target.closest(INTERACTIVE_TARGETS + ', .glass-card > *, .glass-panel > *')) return
    startDrag()
  }

  const onMouseMove = (event: MouseEvent): void => {
    if (!pendingSearchDrag) return
    if (!(event.buttons & 1)) {
      pendingSearchDrag = undefined
      return
    }
    event.preventDefault()
    if (
      Math.hypot(event.screenX - pendingSearchDrag.x, event.screenY - pendingSearchDrag.y) <
      SEARCH_DRAG_THRESHOLD
    )
      return
    pendingSearchDrag = undefined
    startDrag()
  }

  const endDrag = (): void => {
    pendingSearchDrag = undefined
    if (!dragActive) return
    dragActive = false
    void end()
  }

  const onVisibilityChange = (): void => {
    if (document.visibilityState !== 'visible') endDrag()
  }

  window.addEventListener('mousedown', onMouseDown, { capture: true })
  window.addEventListener('mousemove', onMouseMove, { capture: true })
  window.addEventListener('mouseup', endDrag, { capture: true })
  // WebView2 can synthesize blur/visibility events when the snap guide appears.
  // Those must not end a Windows drag; its host owns the drag lifecycle.
  if (!isWindows) {
    window.addEventListener('blur', endDrag)
    document.addEventListener('visibilitychange', onVisibilityChange)
  }
  return () => {
    window.removeEventListener('mousedown', onMouseDown, { capture: true })
    window.removeEventListener('mousemove', onMouseMove, { capture: true })
    window.removeEventListener('mouseup', endDrag, { capture: true })
    window.removeEventListener('blur', endDrag)
    document.removeEventListener('visibilitychange', onVisibilityChange)
    endDrag()
  }
}
