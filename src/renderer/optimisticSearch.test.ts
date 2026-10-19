import { describe, expect, it } from 'vitest'
import type { SearchResult } from '../shared/search'
import { optimisticSearchResults } from './optimisticSearch'

function result(
  id: string,
  title: string,
  subtitle: string,
  category: SearchResult['category'],
  score = 0
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

describe('optimisticSearchResults', () => {
  it('puts the matching app and extension surface above generic files', () => {
    const candidates = [
      result(
        'library',
        'Open ~/Library',
        'Reveal the hidden Library folder',
        'native-command',
        9_999
      ),
      result('file', 'Gemini_Generated_Image.png', '/Users/example/Downloads', 'files'),
      result('summarize', 'Summarize', 'Google Gemini', 'extensions'),
      result('app', 'Gemini 2', '/Applications/Gemini 2.app', 'applications'),
    ]

    expect(optimisticSearchResults(candidates, 'gemini').map((item) => item.id)).toEqual([
      'app',
      'summarize',
      'file',
    ])
  })

  it('matches corrected layouts in the first frame without unrelated cached results', () => {
    const candidates = [
      result('ports', 'Open Ports', 'Port Manager', 'extensions', 50_000),
      result('app', 'VS Code', '/Applications/Visual Studio Code.app', 'applications'),
      result('chrome', 'Chrome', '/Applications/Chrome.app', 'applications'),
    ]

    expect(optimisticSearchResults(candidates, 'МЫ СЩВУ').map((item) => item.id)).toEqual(['app'])
    expect(optimisticSearchResults(candidates, 'vs сщву').map((item) => item.id)).toEqual(['app'])
    expect(optimisticSearchResults(candidates, 'сркщьу').map((item) => item.id)).toEqual(['chrome'])
  })

  it('preserves literal Russian matches and supports the reverse layout', () => {
    const candidates = [
      result('other', 'Open Ports', 'Port Manager', 'extensions', 50_000),
      result('russian', 'Привет', 'Заметка', 'quick-notes'),
    ]

    expect(optimisticSearchResults(candidates, 'Привет').map((item) => item.id)).toEqual(['russian'])
    expect(optimisticSearchResults(candidates, 'ghbdtn').map((item) => item.id)).toEqual(['russian'])
    expect(optimisticSearchResults(candidates, 'Несуществующее')).toEqual([])
    expect(optimisticSearchResults(candidates, '***')).toEqual([])
  })

  it('prefers a literal match over an equally relevant corrected match', () => {
    const candidates = [
      result('corrected', 'Code', 'Application', 'applications', 50_000),
      result('literal', 'Сщву', 'Application', 'applications'),
    ]

    expect(optimisticSearchResults(candidates, 'сщву').map((item) => item.id)).toEqual([
      'literal',
      'corrected',
    ])
  })

  it('does not carry an unrelated high-score result into a typed query', () => {
    const candidates = [
      result('ports', 'Open Ports in Menu Bar', 'Port Manager', 'extensions', 50_000),
      result('app', 'Gemini 2', '/Applications/Gemini 2.app', 'applications'),
    ]

    expect(optimisticSearchResults(candidates, 'gemini').map((item) => item.id)).toEqual(['app'])
  })
})
