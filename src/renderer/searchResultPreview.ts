import type { SearchResult } from '../shared/search'
import { isQuickLookPreviewablePath } from '../shared/quickLook'

export function quickLookPathForSearchResult(
  result: SearchResult | null | undefined
): string | null {
  if (result?.action.type !== 'open-file') return null
  const path = result.action.path.trim()
  return path && isQuickLookPreviewablePath(path) ? path : null
}

export function quickLookPathsForSearchResults(
  results: SearchResult[],
  selectedIndex: number
): string[] {
  if (selectedIndex < 0 || selectedIndex >= results.length) return []
  const selectedPath = quickLookPathForSearchResult(results[selectedIndex])
  if (!selectedPath) return []

  const orderedResults = [...results.slice(selectedIndex), ...results.slice(0, selectedIndex)]
  const seen = new Set<string>()
  return orderedResults.flatMap((result) => {
    const path = quickLookPathForSearchResult(result)
    if (!path || seen.has(path)) return []
    seen.add(path)
    return [path]
  })
}

export function canQuickLookSearchResult(
  visibleResultCount: number,
  navigationActive: boolean,
  selectedPath: string | null
): boolean {
  return Boolean(selectedPath) && (navigationActive || visibleResultCount === 1)
}

export function toggleDeepSearchResultNavigation(
  navigationActive: boolean,
  visibleResultCount: number
): { navigationActive: boolean; selectedIndex: number } {
  if (navigationActive || visibleResultCount === 0) {
    return { navigationActive: false, selectedIndex: -1 }
  }
  return { navigationActive: true, selectedIndex: 0 }
}

export function isPlainSpaceKey(event: {
  key: string
  code: string
  metaKey: boolean
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
}): boolean {
  return (
    (event.key === ' ' || event.code === 'Space') &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.shiftKey
  )
}
