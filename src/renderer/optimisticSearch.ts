import type { SearchResult } from '../shared/search'

function searchTokens(value: string): string[] {
  return value.toLowerCase().match(/[a-z0-9]+/g) ?? []
}

/**
 * Mirrors the backend's relevance shape closely enough for the first frame.
 * In particular, an extension name match ("Google Gemini") is a first-class
 * surface and must beat a generic file whose filename happens to start with
 * the same query.
 */
function optimisticRelevance(item: SearchResult, query: string): number {
  const title = item.title.trim().toLowerCase()
  const subtitle = item.subtitle.trim().toLowerCase()
  const titleTokens = searchTokens(title)

  let lexical = 0
  if (title === query) lexical = 1_000
  else if (title.startsWith(query)) lexical = 900
  else if (titleTokens.some((token) => token === query)) lexical = 850
  else if (title.includes(query)) lexical = 750
  else if (subtitle.includes(query)) lexical = 700
  else lexical = 600

  let surfaceBoost = 0
  if (
    item.category === 'native-command' ||
    item.category === 'commands' ||
    item.category === 'extensions' ||
    item.category === 'applications' ||
    item.category === 'quick-notes'
  ) {
    if (title === query) surfaceBoost = 600
    else if (title.startsWith(query)) surfaceBoost = 420
    else if (titleTokens.some((token) => token.startsWith(query))) surfaceBoost = 300
    else if (title.includes(query)) surfaceBoost = 150
  }

  if (item.category === 'extensions') {
    const extensionName = subtitle.split(' · ')[0] ?? ''
    const compactExtensionName = extensionName.replace(/\s+/g, '')
    if (extensionName === query || compactExtensionName === query) {
      surfaceBoost = Math.max(surfaceBoost, 1_200)
    } else if (extensionName.startsWith(query) || compactExtensionName.startsWith(query)) {
      surfaceBoost = Math.max(surfaceBoost, 800)
    } else if (extensionName.includes(query) || compactExtensionName.includes(query)) {
      surfaceBoost = Math.max(surfaceBoost, 400)
    }
  }

  return lexical + surfaceBoost
}

export function optimisticSearchResults(candidates: SearchResult[], query: string): SearchResult[] {
  const normalizedQuery = query.trim().toLowerCase()
  if (!normalizedQuery) return candidates

  const queryTerms = searchTokens(normalizedQuery)
  return candidates
    .map((item, originalIndex) => {
      const haystack = `${item.title} ${item.subtitle}`.toLowerCase()
      if (!queryTerms.every((term) => haystack.includes(term))) return null

      return {
        item,
        relevance: optimisticRelevance(item, normalizedQuery),
        originalIndex,
      }
    })
    .filter(
      (match): match is { item: SearchResult; relevance: number; originalIndex: number } =>
        match !== null
    )
    .sort(
      (left, right) =>
        right.relevance - left.relevance ||
        right.item.score - left.item.score ||
        left.originalIndex - right.originalIndex
    )
    .map(({ item }) => item)
}
