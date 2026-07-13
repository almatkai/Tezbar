import type { SearchResult } from './search'

export const DEEP_SEARCH_PREFIX = '!'
export const ACTIVATE_DEEP_SEARCH_COMMAND = 'activate-deep-search'
export const DEEP_SEARCH_RESULT_PREFIX = 'deep-search:'

export type ParsedSearchQuery = {
  mode: 'basic' | 'deep'
  query: string
}

/** `!` is a launcher mode prefix, not part of the indexed query. */
export function parseSearchQuery(input: string): ParsedSearchQuery {
  if (input.startsWith(DEEP_SEARCH_PREFIX)) {
    return {
      mode: 'deep',
      query: input.slice(DEEP_SEARCH_PREFIX.length).trim(),
    }
  }
  return { mode: 'basic', query: input.trim() }
}

export function deepSearchInput(query: string): string {
  const trimmed = query.trim()
  return trimmed ? `${DEEP_SEARCH_PREFIX}${trimmed}` : DEEP_SEARCH_PREFIX
}

function tokens(value: string): string[] {
  return value.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []
}

function metadataTokenMatchesQuery(metadataToken: string, queryToken: string): boolean {
  if (metadataToken === queryToken || metadataToken.startsWith(queryToken)) return true
  if (!queryToken.startsWith(metadataToken)) return false

  // A path fragment such as `com` must not make `comienzo` look matched.
  // Reverse prefixes are useful for inflections, but only when most of the
  // requested word is present in the visible metadata.
  const requiredPrefixLength = Math.max(3, Math.ceil(queryToken.length * 0.7))
  return metadataToken.length >= requiredPrefixLength
}

/**
 * A broad OR match is not enough to suppress the deep-search suggestion.
 * Require the visible metadata to cover the whole query, while still accepting
 * a strong fuzzy match for a single misspelled term.
 */
export function hasGoodMetadataMatch(
  query: string,
  results: Array<Pick<SearchResult, 'id' | 'title' | 'subtitle' | 'category' | 'score'>>
): boolean {
  const normalizedQuery = query.trim().toLowerCase()
  const queryTokens = tokens(normalizedQuery)
  if (normalizedQuery.length < 2 || queryTokens.length === 0) return true

  return results.slice(0, 10).some((result) => {
    if (
      result.category === 'knowledge' ||
      result.id.startsWith(DEEP_SEARCH_RESULT_PREFIX) ||
      result.id.startsWith('note-add:')
    ) {
      return false
    }

    const title = result.title.trim().toLowerCase()
    const metadata = `${title} ${result.subtitle.trim().toLowerCase()}`.trim()
    if (title === normalizedQuery || title.startsWith(normalizedQuery)) return true
    if (metadata.includes(normalizedQuery)) return true

    const metadataTokens = tokens(metadata)
    const coversEveryTerm = queryTokens.every((queryToken) =>
      metadataTokens.some((metadataToken) => metadataTokenMatchesQuery(metadataToken, queryToken))
    )
    if (!coversEveryTerm) return false

    return queryTokens.length > 1 || result.score >= 650
  })
}
