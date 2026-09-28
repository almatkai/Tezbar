import { describe, expect, it } from 'vitest'
import { isTezbarThemePreference, resolveTezbarTheme } from './theme'

describe('Tezbar theme preference', () => {
  it('keeps explicit White and Dark modes', () => {
    expect(resolveTezbarTheme('white', true)).toBe('white')
    expect(resolveTezbarTheme('dark', false)).toBe('dark')
  })

  it('resolves System from the operating system appearance', () => {
    expect(resolveTezbarTheme('system', true)).toBe('dark')
    expect(resolveTezbarTheme('system', false)).toBe('white')
  })

  it('rejects unknown persisted values', () => {
    expect(isTezbarThemePreference('white')).toBe(true)
    expect(isTezbarThemePreference('solarized')).toBe(false)
  })
})
