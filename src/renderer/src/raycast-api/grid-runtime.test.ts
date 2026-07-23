import { describe, expect, it } from 'vitest'
import { colorFromGridContent } from './grid-content'

describe('grid color content', () => {
  it('reads adaptive colors from Raycast Grid.Item content', () => {
    expect(
      colorFromGridContent({
        color: { light: '#7CFC00', dark: '#008000', adjustContrast: false },
      })
    ).toBe('#008000')
  })

  it('supports direct and wrapped color values', () => {
    expect(colorFromGridContent({ color: '#0F0' })).toBe('#0F0')
    expect(colorFromGridContent({ value: { color: { light: '#ADFF2F' } } })).toBe('#ADFF2F')
  })
})
