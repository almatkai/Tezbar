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

  it('does not carry an unrelated high-score result into a typed query', () => {
    const candidates = [
      result('ports', 'Open Ports in Menu Bar', 'Port Manager', 'extensions', 50_000),
      result('app', 'Gemini 2', '/Applications/Gemini 2.app', 'applications'),
    ]

    expect(optimisticSearchResults(candidates, 'gemini').map((item) => item.id)).toEqual(['app'])
  })
})
