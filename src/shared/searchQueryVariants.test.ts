import { describe, expect, it } from 'vitest'
import { searchQueryVariants } from './searchQueryVariants'

describe('searchQueryVariants', () => {
  it('corrects Russian keys to English without discarding the literal query', () => {
    expect(searchQueryVariants('  МЫ СЩВУ  ')).toEqual(['мы сщву', 'vs code'])
    expect(searchQueryVariants('сркщьу')).toEqual(['сркщьу', 'chrome'])
    expect(searchQueryVariants('ЕУДУПКФЬ')).toEqual(['еудупкфь', 'telegram'])
  })

  it('also corrects English keys to Russian', () => {
    expect(searchQueryVariants('ghbdtn')).toEqual(['ghbdtn', 'привет'])
    expect(searchQueryVariants('Gjиcr')).toContain('поиск')
  })

  it('retains already-correct words in mixed-layout input', () => {
    expect(searchQueryVariants('МЫ code')).toContain('vs code')
    expect(searchQueryVariants('vs сщву')).toContain('vs code')
  })

  it('handles Russian letter keys on punctuation positions', () => {
    expect(searchQueryVariants('ёхъжэбю')).toContain("`[];',.")
    expect(searchQueryVariants('`[];\',.a')).toContain('ёхъжэбюф')
  })

  it('does not expand empty, numeric, or punctuation-only queries', () => {
    expect(searchQueryVariants('  ')).toEqual([])
    expect(searchQueryVariants('123')).toEqual(['123'])
    expect(searchQueryVariants('! / *')).toEqual(['! / *'])
  })

  it('keeps real Russian and English queries as the first choice', () => {
    expect(searchQueryVariants('Поиск документов')[0]).toBe('поиск документов')
    expect(searchQueryVariants('VS Code')[0]).toBe('vs code')
    expect(searchQueryVariants('你好')).toEqual(['你好'])
  })
})
