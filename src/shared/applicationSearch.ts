// Shared by indexed search, first-frame filtering, and the Deep Search fallback.
// These are app names/aliases, not general-purpose translation of user queries.
const APPLICATION_ALIASES: Readonly<Record<string, readonly string[]>> = {
  'visual studio code': ['vscode', 'vsc', 'code', 'vs code', 'вс код', 'вскод'],
  'google chrome': ['chrome', 'хром', 'гугл хром'],
  chrome: ['хром', 'гугл хром'],
  telegram: ['телеграм', 'телеграмм'],
  discord: ['дискорд'],
  docker: ['докер', 'доккер'],
  safari: ['сафари'],
  raycast: ['рейкаст'],
  claude: ['клод', 'клауд'],
  xcode: ['икскод'],
  finder: ['files', 'filemanager', 'файндер'],
  terminal: ['term', 'shell', 'терминал'],
  'quicktime player': ['quicktime', 'qt', 'qtp'],
  'activity monitor': ['activity', 'taskmanager', 'task monitor', 'мониторинг системы'],
}

function normalize(value: string): string {
  return value.trim().toLowerCase().replace(/\s+/g, ' ')
}

export function applicationSearchAliases(appName: string): readonly string[] {
  const name = normalize(appName)
  return Object.hasOwn(APPLICATION_ALIASES, name) ? (APPLICATION_ALIASES[name] ?? []) : []
}

/** Match a single query variant, requiring the whole query rather than an OR hit.
 * Layout variants and their literal-first penalty are applied by the caller. */
export function applicationNameMatchScore(appName: string, query: string): number {
  const normalizedQuery = normalize(query)
  const queryTokens = normalizedQuery.match(/[\p{L}\p{N}]+/gu) ?? []
  if (!queryTokens.length) return 0

  let best = 0
  for (const name of [normalize(appName), ...applicationSearchAliases(appName)]) {
    if (name === normalizedQuery) return 1
    if (name.startsWith(normalizedQuery)) {
      best = Math.max(best, 0.92)
      continue
    }
    const nameTokens = name.match(/[\p{L}\p{N}]+/gu) ?? []
    if (queryTokens.every((term) => nameTokens.some((word) => word.startsWith(term)))) {
      best = Math.max(best, 0.9)
    }
  }
  return best
}
