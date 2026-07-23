function wordTokens(value: string): string[] {
  return (
    value
      .toLocaleLowerCase()
      .normalize('NFKC')
      .match(/[\p{L}\p{N}]+/gu) ?? []
  )
}

export function tokenizeQuery(query: string): string[] {
  return wordTokens(query)
}

export function lexicalScore(text: string, query: string): number {
  const t = text.toLowerCase()
  const q = query.toLowerCase().trim()
  if (!q) return 0
  if (t === q) return 1
  if (t.startsWith(q)) return 0.9
  if (t.includes(q)) return 0.75

  const tokens = tokenizeQuery(q)
  if (tokens.length === 0) return 0

  let matched = 0
  for (const token of tokens) {
    if (t.includes(token)) matched += 1
  }
  return matched / tokens.length / 1.5
}

function searchableTokens(text: string): string[] {
  return wordTokens(text)
}

export function levenshteinDistance(left: string, right: string): number {
  if (left === right) return 0
  if (!left) return right.length
  if (!right) return left.length

  const a = left.toLowerCase()
  const b = right.toLowerCase()
  const prev: number[] = Array.from({ length: b.length + 1 }, (_, i) => i)
  const cur: number[] = new Array(b.length + 1).fill(0)

  for (let i = 1; i <= a.length; i += 1) {
    cur[0] = i
    for (let j = 1; j <= b.length; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost)
    }
    for (let j = 0; j <= b.length; j += 1) {
      prev[j] = cur[j]
    }
  }

  return prev[b.length]
}

function tokenSimilarity(candidate: string, query: string): number {
  if (!candidate || !query) return 0
  if (candidate === query) return 1
  if (candidate.startsWith(query)) return 0.92
  if (query.startsWith(candidate) && candidate.length >= 3) return 0.82
  // Reverse containment creates disastrous false positives: "gemini"
  // contains common candidate tokens such as "in" and "min". Search text
  // may contain the user's query, but not the other way around. Prefix
  // handling above still supports short intentional queries like "op".
  if (query.length >= 3 && candidate.includes(query)) {
    return 0.74
  }

  const distance = levenshteinDistance(candidate, query)
  const length = Math.max(candidate.length, query.length)
  const maxDistance = Math.max(1, Math.floor(length * 0.38))
  if (distance > maxDistance) return 0

  return Math.max(0, 1 - distance / length)
}

export function fuzzySimilarityScore(text: string, query: string): number {
  const queryTokens = searchableTokens(query)
  if (queryTokens.length === 0) return 0

  const candidateTokens = searchableTokens(text)
  if (candidateTokens.length === 0) return 0

  let total = 0
  for (const queryToken of queryTokens) {
    let best = 0
    for (const candidateToken of candidateTokens) {
      best = Math.max(best, tokenSimilarity(candidateToken, queryToken))
    }
    total += best
  }

  const average = total / queryTokens.length
  return average >= 0.45 ? average : 0
}

export function buildFtsQuery(query: string): string {
  const tokens = wordTokens(query)
  if (tokens.length === 0) return ''
  // A one-character prefix such as `n*` can match most of a large file
  // corpus and force FTS5 to rank thousands of rows for a transient
  // keystroke. Exact single-character terms remain supported without the
  // pathological fan-out; the renderer's local candidate cache covers the
  // normal one-character launcher experience.
  return tokens.map((token) => (token.length === 1 ? `"${token}"` : `${token}*`)).join(' OR ')
}

/**
 * Content indexes are much larger than launcher metadata indexes. Avoid broad
 * one- and two-character prefixes, and require all meaningful terms so a
 * partially typed query cannot force FTS5 to rank most of the corpus.
 */
export function buildContentFtsQuery(query: string): string {
  const tokens = wordTokens(query)
  return tokens
    .filter((token) => token.length >= 3)
    .map((token) => (token.length >= 4 ? `${token}*` : token))
    .join(' AND ')
}
