import { describe, expect, it } from 'vitest'
import { applicationNameMatchScore, applicationSearchAliases } from './applicationSearch'

describe('application name matching', () => {
  it.each([
    ['Visual Studio Code', 'vs code'],
    ['Visual Studio Code', 'vscode'],
    ['Visual Studio Code', 'вс код'],
    ['Google Chrome', 'хром'],
    ['Telegram', 'телеграм'],
    ['Discord', 'дискорд'],
    ['Safari', 'сафари'],
  ])('recognizes %s by the alias %s', (name, query) => {
    expect(applicationNameMatchScore(name, query)).toBe(1)
  })

  it('matches complete name tokens, including partial typing, without dropping terms', () => {
    expect(applicationNameMatchScore('Google Chrome', 'chrome')).toBeGreaterThan(0)
    expect(applicationNameMatchScore('Visual Studio Code', 'VS   CO')).toBeGreaterThan(0)
    expect(applicationNameMatchScore('Visual Studio Code', 'studio code')).toBeGreaterThan(0)
    expect(applicationNameMatchScore('Google Chrome', 'no chrome')).toBe(0)
    expect(applicationNameMatchScore('Visual Studio Code', 'vs unrelated')).toBe(0)
    expect(applicationNameMatchScore('Google Chrome', '***')).toBe(0)
  })

  it('does not inherit object keys or add aliases to unknown applications', () => {
    expect(applicationSearchAliases('constructor')).toEqual([])
    expect(applicationSearchAliases('Unrelated App')).toEqual([])
    expect(applicationNameMatchScore('Unrelated App', 'хром')).toBe(0)
    expect(applicationNameMatchScore('Привет', 'привет')).toBe(1)
  })
})
