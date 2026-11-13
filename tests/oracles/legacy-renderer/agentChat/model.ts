/** Random id generator for chat sessions + turns. */
export function makeChatId(): string {
  return Math.random().toString(36).slice(2, 10) + Date.now().toString(36)
}

export function summarizeChatTitle(firstUserText: string): string {
  const firstLine = firstUserText.split('\n').find((l) => l.trim()) ?? ''
  const cleaned = firstLine.trim()
  if (!cleaned) return 'New chat'
  return cleaned.length > 64 ? cleaned.slice(0, 61) + '…' : cleaned
}

/** Placeholder for the history panel: an empty list while the backend is still
 *  answering is not the same as having no saved conversations. */
export function historyPlaceholder(loading: boolean): string {
  return loading ? 'Loading conversations…' : 'No saved conversations yet.'
}
