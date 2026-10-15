import { readFileSync } from 'node:fs'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import type { LauncherLayout, LauncherViewport } from '../shared/windowLayout'
import { LauncherFrame } from './LauncherFrame'
import { bindLauncherViewport } from './launcherViewport'

function fixture() {
  const styles = new Map<string, string>()
  const offViewport = vi.fn()
  const offLayout = vi.fn()
  const viewportListeners: Array<(frame: LauncherViewport) => void> = []
  const layoutListeners: Array<(frame: LauncherLayout) => void> = []
  let resolveInitial!: (layout: LauncherLayout) => void
  const initial = new Promise<LauncherLayout>((resolve) => {
    resolveInitial = resolve
  })
  const stop = bindLauncherViewport(
    {
      style: {
        setProperty: (name, value) => {
          styles.set(name, value ?? '')
        },
      },
    },
    {
      getLauncherLayout: () => initial,
      onLauncherViewportChanged: (listener) => {
        viewportListeners.push(listener)
        return offViewport
      },
      onLauncherLayoutChanged: (listener) => {
        layoutListeners.push(listener)
        return offLayout
      },
    }
  )
  return {
    styles,
    offViewport,
    offLayout,
    resolveInitial,
    stop,
    frame: (value: LauncherViewport) => viewportListeners.forEach((listener) => listener(value)),
    layout: (value: LauncherLayout) => layoutListeners.forEach((listener) => listener(value)),
  }
}
const initial: LauncherLayout = {
  displayId: 'monitor',
  displayName: 'Monitor',
  width: 760,
  height: 640,
  zoom: 1,
}

describe('launcher live-resize viewport', () => {
  it('moves the shell bottom immediately on every frame, before mouse release or a persisted snapshot', () => {
    const f = fixture()
    for (const height of [600, 560, 500, 460, 520, 580]) {
      f.frame({ width: 760, height, zoom: 1 })
      expect(f.styles.get('--launcher-height')).toBe(`${height}px`)
    }
    f.stop()
  })
  it('accounts for page zoom separately from OS-logical dimensions', () => {
    const f = fixture()
    f.frame({ width: 900, height: 600, zoom: 1.5 })
    expect(f.styles.get('--launcher-width')).toBe('600px')
    expect(f.styles.get('--launcher-height')).toBe('400px')
    f.frame({ width: 600, height: 450, zoom: 0.75 })
    expect(f.styles.get('--launcher-height')).toBe('600px')
    f.stop()
  })
  it('does not let a late initial snapshot undo the latest resize frame', async () => {
    const f = fixture()
    f.frame({ width: 600, height: 460, zoom: 1 })
    f.resolveInitial(initial)
    await Promise.resolve()
    expect(f.styles.get('--launcher-height')).toBe('460px')
    f.stop()
  })
  it('initializes from the native snapshot and responds to scale/profile changes', async () => {
    const f = fixture()
    f.resolveInitial(initial)
    await Promise.resolve()
    expect(f.styles.get('--launcher-height')).toBe('640px')
    f.layout({ ...initial, zoom: 2, width: 1200, height: 800 })
    expect(f.styles.get('--launcher-height')).toBe('400px')
    f.stop()
  })
  it('ignores invalid frames and updates arriving after unmount', async () => {
    const f = fixture()
    f.frame({ width: 760, height: 460, zoom: 1 })
    f.frame({ width: 760, height: Number.NaN, zoom: 1 })
    f.frame({ width: 760, height: 400, zoom: 0 })
    expect(f.styles.get('--launcher-height')).toBe('460px')
    f.stop()
    f.frame({ width: 760, height: 300, zoom: 1 })
    f.resolveInitial(initial)
    await Promise.resolve()
    expect(f.styles.get('--launcher-height')).toBe('460px')
    expect(f.offViewport).toHaveBeenCalledOnce()
    expect(f.offLayout).toHaveBeenCalledOnce()
  })
  it('keeps the actual launcher shell driven by the live frame, without a height transition', () => {
    const markup = renderToStaticMarkup(createElement(LauncherFrame, { children: 'Footer' }))
    expect(markup).toContain('var(--launcher-height, 100vh)')
    expect(markup).not.toContain('h-screen')
    const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    const shellRule = css.match(/\.glass-shell\s*\{([^}]*)\}/)?.[1]
    expect(shellRule).toContain('transition: none')
  })
})
