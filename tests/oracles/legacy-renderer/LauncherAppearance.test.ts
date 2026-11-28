import { describe, expect, it } from 'vitest'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import {
  LauncherAppearanceButton,
  LauncherAppearanceSettings,
  LauncherResizeHandles,
} from './LauncherAppearance'

describe('launcher appearance controls', () => {
  it('renders only a dot, with a larger keyboard-accessible hit target', () => {
    const markup = renderToStaticMarkup(createElement(LauncherAppearanceButton))
    expect(markup).toContain('aria-label="Launcher size and interface scale"')
    expect(markup).toContain('aria-haspopup="dialog"')
    expect(markup).toContain('h-1.5 w-1.5 rounded-full')
    expect(markup).toContain('h-7 w-6')
    expect(markup).not.toContain('%')
    expect(markup).not.toContain('<svg')
  })
  it('renders eight resize targets without making them keyboard traps', () => {
    const markup = renderToStaticMarkup(createElement(LauncherResizeHandles))
    expect(markup.match(/aria-hidden="true"/g)).toHaveLength(8)
    expect(markup).toContain('cursor-nwse-resize')
    expect(markup).toContain('cursor-nesw-resize')
    expect(markup).toContain('cursor-ns-resize')
    expect(markup).toContain('cursor-ew-resize')
    expect(markup).not.toContain('tabindex')
  })
  it('offers scale, exact dimensions and a per-display reset', () => {
    const markup = renderToStaticMarkup(createElement(LauncherAppearanceSettings))
    expect(markup).toContain('75%')
    expect(markup).toContain('200%')
    expect(markup).toContain('Launcher width')
    expect(markup).toContain('Launcher height')
    expect(markup).toContain('Reset size &amp; scale for this display')
    expect(markup).toContain('remembered for each display')
  })
})
