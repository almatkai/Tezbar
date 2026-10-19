import type { SearchResult } from '../shared/search'
import { applicationNameMatchScore } from '../shared/applicationSearch'
import { searchQueryVariants } from '../shared/searchQueryVariants'

function searchTokens(value: string): string[] {
  return value.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []
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

  const variants = searchQueryVariants(normalizedQuery).map((variant) => ({
    query: variant,
    terms: searchTokens(variant),
  }))
  return candidates
    .map((item, originalIndex) => {
      const haystack = `${item.title} ${item.subtitle}`.toLowerCase()
      let relevance = -1
      for (const [index, variant] of variants.entries()) {
        const appMatch =
          item.category === 'applications'
            ? applicationNameMatchScore(item.title, variant.query)
            : 0
        if (
          !variant.terms.length ||
          (!appMatch && !variant.terms.every((term) => haystack.includes(term)))
        ) {
          continue
        }
        relevance = Math.max(
          relevance,
          Math.max(optimisticRelevance(item, variant.query), appMatch * 1_600) *
            (index === 0 ? 1 : 0.9)
        )
      }
      if (relevance < 0) return null

      return { item, relevance, originalIndex }
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
