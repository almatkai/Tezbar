import type { SearchResult } from './search'
import { applicationNameMatchScore } from './applicationSearch'
import { searchQueryVariants } from './searchQueryVariants'

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
  return deepSearchDraftInput(trimmed)
}

/** Preserve the editable text while keeping the hidden Deep Search sentinel. */
export function deepSearchDraftInput(query: string): string {
  return `${DEEP_SEARCH_PREFIX}${query}`
}

/** Stable backend input: whitespace-only edits do not represent a new search. */
export function searchRequestInput(input: string): string {
  const parsed = parseSearchQuery(input)
  return parsed.mode === 'deep' ? deepSearchInput(parsed.query) : parsed.query
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
 * Require metadata or a known app alias to cover a whole query variant.
 * Layout corrections must use the same acceptance rules as literal input.
 */
export function hasGoodMetadataMatch(
  query: string,
  results: Array<Pick<SearchResult, 'id' | 'title' | 'subtitle' | 'category' | 'score'>>
): boolean {
  const normalizedQuery = query.trim().toLowerCase()
  const queryTokens = tokens(normalizedQuery)
  if (normalizedQuery.length < 2 || queryTokens.length === 0) return true

  const variants = searchQueryVariants(normalizedQuery)
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
    const metadataTokens = tokens(metadata)
    return variants.some((variant) => {
      if (result.category === 'applications' && applicationNameMatchScore(title, variant) > 0) {
        return true
      }
      if (title === variant || title.startsWith(variant) || metadata.includes(variant)) return true

      const variantTokens = tokens(variant)
      if (!variantTokens.length) return false
      const coversEveryTerm = variantTokens.every((queryToken) =>
        metadataTokens.some((metadataToken) => metadataTokenMatchesQuery(metadataToken, queryToken))
      )
      return coversEveryTerm && (variantTokens.length > 1 || result.score >= 650)
    })
  })
}

/** Build the zero-wait fallback shown when launcher metadata has no close match. */
export function buildDeepSearchRecommendation(
  query: string,
  results: Array<Pick<SearchResult, 'id' | 'title' | 'subtitle' | 'category' | 'score'>>
): SearchResult | null {
  const trimmed = query.trim()
  if (trimmed.length < 3 || hasGoodMetadataMatch(trimmed, results)) return null

  const bestCandidateScore = results.reduce((best, candidate) => Math.max(best, candidate.score), 0)
  return {
    id: `${DEEP_SEARCH_RESULT_PREFIX}${encodeURIComponent(trimmed.toLowerCase())}`,
    title: `Deep Search “${trimmed.slice(0, 72)}”`,
    subtitle: 'No strong metadata match · Search inside indexed file contents',
    category: 'knowledge',
    score: Math.max(1_200, bestCandidateScore + 1),
    action: {
      type: 'invoke-command',
      commandId: ACTIVATE_DEEP_SEARCH_COMMAND,
      payload: { query: trimmed },
    },
  }
}
