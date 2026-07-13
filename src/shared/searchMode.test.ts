import { describe, expect, it } from 'vitest'
import type { SearchResult } from './search'
import { deepSearchInput, hasGoodMetadataMatch, parseSearchQuery } from './searchMode'

function result(
  id: string,
  title: string,
  subtitle: string,
  category: SearchResult['category'] = 'files',
  score = 700
): SearchResult {
  return {
    id,
    title,
    subtitle,
    category,
    score,
    action: { type: 'copy-text', text: title },
  }
}

describe('search modes', () => {
  it('treats a leading bang as deep search and removes it from the indexed query', () => {
    expect(parseSearchQuery('! quarterly revenue ')).toEqual({
      mode: 'deep',
      query: 'quarterly revenue',
    })
    expect(parseSearchQuery(' ! quarterly revenue ')).toEqual({
      mode: 'basic',
      query: '! quarterly revenue',
    })
    expect(parseSearchQuery('quarterly revenue')).toEqual({
      mode: 'basic',
      query: 'quarterly revenue',
    })
    expect(deepSearchInput(' quarterly revenue ')).toBe('!quarterly revenue')
  })

  it('accepts a complete metadata match', () => {
    expect(
      hasGoodMetadataMatch('quarterly revenue', [
        result('file:report', 'Q3 quarterly report.pdf', '/Documents/revenue'),
      ])
    ).toBe(true)
  })

  it('recommends deep search for weak partial matches and ignores synthetic rows', () => {
    expect(
      hasGoodMetadataMatch('quarterly revenue', [
        result('file:quarterly', 'quarterly-notes.txt', '/Documents'),
        result('note-add:quarterly revenue', 'Add quick note: quarterly revenue', 'Quick notes'),
        result(
          'deep-search:quarterly-revenue',
          'Deep Search “quarterly revenue”',
          'Search inside indexed file contents',
          'knowledge',
          10_000
        ),
      ])
    ).toBe(false)
  })

  it('does not treat short path fragments as a match for a punctuated Unicode query', () => {
    expect(
      hasGoodMetadataMatch('¡Comienzo!', [
        result(
          'file:person-promoter',
          'PersonPromoter',
          '/Pictures/Photos Library.photoslibrary/private/com.apple.mediaanalysis/cache'
        ),
        result(
          'snippet:commit',
          'Conventional commit message (full template)',
          'Commit template',
          'snippets'
        ),
        result('quick-link:google', 'Google Search', 'Quick link', 'quick-links'),
        result('note-add:comienzo', 'Add quick note: ¡Comienzo!', 'Quick notes', 'quick-notes'),
      ])
    ).toBe(false)
  })

  it('still accepts a substantial reverse-prefix metadata match', () => {
    expect(
      hasGoodMetadataMatch('quarterly', [
        result('file:quarter', 'Quarter report.pdf', '/Documents'),
      ])
    ).toBe(true)
  })
})
