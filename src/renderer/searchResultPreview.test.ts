import { describe, expect, it } from 'vitest'
import type { SearchResult } from '../shared/search'
import {
  canQuickLookSearchResult,
  isPlainSpaceKey,
  quickLookPathForSearchResult,
  quickLookPathsForSearchResults,
  toggleDeepSearchResultNavigation,
} from './searchResultPreview'

function result(action: SearchResult['action']): SearchResult {
  return {
    id: 'result',
    title: 'Result',
    subtitle: '',
    category: 'files',
    score: 1,
    action,
  }
}

describe('search result Quick Look', () => {
  it('returns paths only for file-backed results that Quick Look supports', () => {
    expect(
      quickLookPathForSearchResult(result({ type: 'open-file', path: '/tmp/report.pdf' }))
    ).toBe('/tmp/report.pdf')
    expect(quickLookPathForSearchResult(result({ type: 'copy-text', text: 'hello' }))).toBeNull()
    expect(
      quickLookPathForSearchResult(result({ type: 'open-file', path: '/tmp/review.docx' }))
    ).toBe('/tmp/review.docx')
    expect(
      quickLookPathForSearchResult(result({ type: 'open-file', path: '/tmp/archive.zip' }))
    ).toBeNull()
    expect(quickLookPathForSearchResult(result({ type: 'open-file', path: '/tmp/scan.PDF' }))).toBe(
      '/tmp/scan.PDF'
    )
  })

  it('recognizes only an unmodified Space key', () => {
    expect(
      isPlainSpaceKey({
        key: ' ',
        code: 'Space',
        metaKey: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
      })
    ).toBe(true)
    expect(
      isPlainSpaceKey({
        key: ' ',
        code: 'Space',
        metaKey: true,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
      })
    ).toBe(false)
  })

  it('allows a sole previewable result without requiring arrow-key navigation', () => {
    expect(canQuickLookSearchResult(1, false, '/tmp/only-result.docx')).toBe(true)
    expect(canQuickLookSearchResult(2, false, '/tmp/first-result.jpg')).toBe(false)
    expect(canQuickLookSearchResult(2, true, '/tmp/first-result.jpg')).toBe(true)
    expect(canQuickLookSearchResult(1, false, null)).toBe(false)
  })

  it('toggles Deep Search between an unselected input and the first result', () => {
    expect(toggleDeepSearchResultNavigation(false, 2)).toEqual({
      navigationActive: true,
      selectedIndex: 0,
    })
    expect(toggleDeepSearchResultNavigation(true, 2)).toEqual({
      navigationActive: false,
      selectedIndex: -1,
    })
    expect(toggleDeepSearchResultNavigation(false, 0)).toEqual({
      navigationActive: false,
      selectedIndex: -1,
    })
  })

  it('orders previewable results from the selected item for native navigation', () => {
    const results = [
      result({ type: 'open-file', path: '/tmp/first.jpg' }),
      result({ type: 'open-file', path: '/tmp/archive.zip' }),
      result({ type: 'open-file', path: '/tmp/second.pdf' }),
      result({ type: 'open-file', path: '/tmp/third.png' }),
    ]

    expect(quickLookPathsForSearchResults(results, 2)).toEqual([
      '/tmp/second.pdf',
      '/tmp/third.png',
      '/tmp/first.jpg',
    ])
    expect(quickLookPathsForSearchResults(results, 1)).toEqual([])
  })
})
