import Fuse from 'fuse.js'
import { EMOJI_DATA, type EmojiCategory, type EmojiEntry } from './emojiData'

const STOP_WORDS = new Set(['the', 'a', 'an', 'of', 'to', 'and', 'for', 'is', 'it'])

const SYNONYMS: Record<string, string[]> = {
  angry: ['mad', 'furious', 'rage', 'annoyed'],
  happy: ['joy', 'smile', 'glad', 'cheerful'],
  sad: ['cry', 'upset', 'down', 'depressed'],
  love: ['heart', 'romance', 'affection', 'crush'],
  cool: ['awesome', 'nice', 'fire', 'dope'],
  sus: ['suspicious', 'doubt', 'weird', 'sketchy'],
  laugh: ['lol', 'haha', 'lmao', 'funny'],
  party: ['celebrate', 'birthday', 'festive', 'hype'],
  tired: ['sleepy', 'exhausted', 'drained', 'fatigue'],
  scared: ['afraid', 'fear', 'anxious', 'panic'],
  fire: ['hot', 'lit', 'flame', 'trending'],
}

const fuse = new Fuse(EMOJI_DATA, {
  keys: [
    { name: 'name', weight: 0.35 },
    { name: 'keywords', weight: 0.4 },
    { name: 'category', weight: 0.15 },
    { name: 'char', weight: 0.1 },
  ],
  threshold: 0.28,
  ignoreLocation: true,
  includeScore: true,
})

function normalize(input: string): string {
  return input
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^\p{L}\p{N}\s]/gu, ' ')
}

function stem(token: string): string {
  if (token.length <= 3) return token
  if (token.endsWith('ing') && token.length > 5) return token.slice(0, -3)
  if (token.endsWith('ed') && token.length > 4) return token.slice(0, -2)
  if (token.endsWith('es') && token.length > 4) return token.slice(0, -2)
  if (token.endsWith('s') && token.length > 3) return token.slice(0, -1)
  return token
}

function preprocessQuery(query: string): string {
  const normalized = normalize(query)
  const rawTokens = normalized.split(/\s+/).map((v) => v.trim()).filter(Boolean)
  const expanded: string[] = []
  for (const token of rawTokens) {
    if (STOP_WORDS.has(token)) continue
    const stemmed = stem(token)
    expanded.push(token, stemmed)
    const related = SYNONYMS[token] ?? SYNONYMS[stemmed]
    if (related) expanded.push(...related)
  }
  return Array.from(new Set(expanded)).join(' ')
}

function rankResult(query: string, item: EmojiEntry, initialScore: number): number {
  const normalizedQuery = normalize(query).trim()
  const normalizedName = normalize(item.name)
  const normalizedKeywords = item.keywords.map((kw) => normalize(kw))

  let score = initialScore
  if (normalizedName === normalizedQuery) score *= 0.2
  else if (normalizedName.startsWith(normalizedQuery)) score *= 0.4
  if (normalizedKeywords.includes(normalizedQuery)) score *= 0.5
  return score
}

export function searchEmojis(query: string, category: EmojiCategory | 'All'): EmojiEntry[] {
  const q = query.trim()
  if (!q) {
    return category === 'All' ? EMOJI_DATA : EMOJI_DATA.filter((entry) => entry.category === category)
  }
  const assembled = preprocessQuery(q)
  const ranked = fuse.search(assembled).map((result) => ({
    item: result.item,
    score: rankResult(q, result.item, result.score ?? 1),
  }))
  ranked.sort((a, b) => a.score - b.score)
  const filtered = category === 'All' ? ranked : ranked.filter((entry) => entry.item.category === category)
  return filtered.map((entry) => entry.item)
}
