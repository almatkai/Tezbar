import { useEffect } from 'react'
import { launcherZoomAfterKey } from '../shared/windowLayout'

/** Leave Terminal's text zoom intact; Alt opts into whole-window zoom. */
export function useLauncherScaleShortcuts(surface: string): void {
  useEffect(() => {
    let zoom = window.tezbar.getWindowZoomFactor()
    let pending = Promise.resolve()
    let queued = 0
    const off = window.tezbar.onLauncherLayoutChanged((layout) => {
      if (queued === 0) zoom = layout.zoom
    })
    void window.tezbar
      .getLauncherLayout()
      .then((layout) => {
        if (queued === 0) zoom = layout.zoom
      })
      .catch(() => undefined)
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || (surface === 'terminal' && !event.altKey)) return
      if (event.shiftKey && event.key !== '+') return
      const next = launcherZoomAfterKey(zoom, event.key)
      if (next === null) return
      event.preventDefault()
      event.stopImmediatePropagation()
      zoom = next
      queued += 1
      pending = pending
        .then(async () => {
          const layout = await window.tezbar.setLauncherLayout({ zoom: next })
          if (queued === 1) zoom = layout.zoom
        })
        .catch((error) => console.warn('[Scale]', error))
        .finally(() => {
          queued -= 1
        })
    }
    window.addEventListener('keydown', onKey, true)
    return () => {
      off()
      window.removeEventListener('keydown', onKey, true)
    }
  }, [surface])
}
