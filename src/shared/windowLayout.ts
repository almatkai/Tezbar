export type LauncherLayout = {
  displayId: string
  displayName: string
  /** Logical window pixels (OS display scaling is applied separately). */
  width: number
  height: number
  /** Page zoom, not devicePixelRatio or native window dimensions. */
  zoom: number
}

/** Live native frame dimensions, emitted before persistence is debounced. */
export type LauncherViewport = Pick<LauncherLayout, 'width' | 'height' | 'zoom'>

export type LauncherLayoutPatch = Partial<Pick<LauncherLayout, 'width' | 'height' | 'zoom'>> & {
  reset?: boolean
}

export type ResizeEdge =
  | 'North'
  | 'NorthEast'
  | 'East'
  | 'SouthEast'
  | 'South'
  | 'SouthWest'
  | 'West'
  | 'NorthWest'

export function launcherZoomAfterKey(zoom: number, key: string): number | null {
  if (key === '0') return 1
  if (key !== '+' && key !== '=' && key !== '-') return null
  return Math.max(0.75, Math.min(2, Math.round((zoom + (key === '-' ? -0.1 : 0.1)) * 100) / 100))
}
