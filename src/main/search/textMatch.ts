export function tokenizeQuery(query: string): string[] {
  return query
    .toLowerCase()
    .trim()
    .split(/\s+/)
    .map((token) => token.replace(/[^a-z0-9._-]/g, ''))
    .filter(Boolean)
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

export function buildFtsQuery(query: string): string {
  const tokens = query
    .toLowerCase()
    .match(/[a-z0-9]+/g) ?? []
  if (tokens.length === 0) return ''
  return tokens.map((token) => `${token}*`).join(' OR ')
}
