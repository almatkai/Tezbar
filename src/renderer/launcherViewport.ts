import type { RaymesApi } from '../shared/desktop-api'
import type { LauncherViewport } from '../shared/windowLayout'

type ViewportApi = Pick<
  RaymesApi,
  'getLauncherLayout' | 'onLauncherLayoutChanged' | 'onLauncherViewportChanged'
>

/** WKWebView can defer vh/innerHeight updates until mouse release during a native resize. */
export function bindLauncherViewport(
  element: { style: Pick<CSSStyleDeclaration, 'setProperty'> },
  api: ViewportApi
): () => void {
  let disposed = false
  let receivedFrame = false
  const apply = ({ width, height, zoom }: LauncherViewport): void => {
    if (disposed || ![width, height, zoom].every((value) => Number.isFinite(value) && value > 0))
      return
    // Window dimensions are OS-logical pixels; page zoom is a separate scale.
    // Update CSS directly, without rerendering all launcher results each frame.
    element.style.setProperty('--launcher-width', `${width / zoom}px`)
    element.style.setProperty('--launcher-height', `${height / zoom}px`)
  }
  const receive = (frame: LauncherViewport): void => {
    receivedFrame = true
    apply(frame)
  }
  const offViewport = api.onLauncherViewportChanged(receive)
  const offLayout = api.onLauncherLayoutChanged(receive)
  void api
    .getLauncherLayout()
    .then((layout) => {
      if (!receivedFrame) apply(layout)
    })
    .catch(() => undefined)
  return () => {
    disposed = true
    offViewport()
    offLayout()
  }
}
