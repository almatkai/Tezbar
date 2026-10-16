import { useEffect, useRef, type ReactNode } from 'react'
import { bindLauncherViewport } from './launcherViewport'

export function LauncherFrame({
  children,
  terminal = false,
  launcher = false,
  sheet = false,
}: {
  children: ReactNode
  terminal?: boolean
  /** Main command surface: the single-sheet launcher look. */
  launcher?: boolean
  /** Edge-to-edge sheet surfaces drop the window inset. */
  sheet?: boolean
}): JSX.Element {
  const root = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (root.current) return bindLauncherViewport(root.current, window.tezbar)
  }, [])
  return (
    <div
      ref={root}
      style={{ width: 'var(--launcher-width, 100vw)', height: 'var(--launcher-height, 100vh)' }}
      className={[
        'glass-shell drag-region relative flex min-h-0 select-none',
        sheet ? 'p-0' : 'p-2',
        launcher ? 'glass-shell--launcher' : '',
        terminal ? 'glass-shell--terminal' : '',
      ]
        .filter(Boolean)
        .join(' ')}
    >
      {children}
    </div>
  )
}
