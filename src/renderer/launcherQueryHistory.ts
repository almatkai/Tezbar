export const LAUNCHER_QUERY_HISTORY_LIMIT = 50

/**
 * Returns the value that should be saved when the launcher submits.
 * Terminal commands are intentionally excluded because the terminal owns its
 * own command history. AI prompts are normalized to the launcher's leading
 * space form so recalling them restores AI mode as well as the text.
 */
export function launcherQueryHistoryEntry(value: string, terminalMode: boolean): string | null {
  if (terminalMode) return null

  const query = value.trim()
  if (!query) return null

  const isAiMode = value.startsWith(' ') || value.endsWith('  ')
  return isAiMode ? ` ${query}` : query
}

export function addLauncherQueryHistoryEntry(
  history: string[],
  entry: string,
  limit = LAUNCHER_QUERY_HISTORY_LIMIT
): string[] {
  const normalizedLimit = Math.max(0, Math.floor(limit))
  if (normalizedLimit === 0) return []
  return [entry, ...history.filter((item) => item !== entry)].slice(0, normalizedLimit)
}

export function parseLauncherQueryHistory(raw: string | null): string[] {
  if (!raw) return []

  try {
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    return parsed
      .filter(
        (item): item is string =>
          typeof item === 'string' && item.length > 0 && item.trim().length > 0
      )
      .slice(0, LAUNCHER_QUERY_HISTORY_LIMIT)
  } catch {
    return []
  }
}
