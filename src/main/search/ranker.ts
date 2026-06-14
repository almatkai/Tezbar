import type { SearchCategory } from '../../shared/search'

type RankFeatures = {
  lexical: number
  recencyMs: number
  frequency: number
  successRate: number
  category: SearchCategory
  fuzzyDistance?: number
  popularity?: number
}

const CATEGORY_PRIOR: Record<SearchCategory, number> = {
  applications: 0.72,
  files: 0.6,
  clipboard: 0.45,
  /** Was 0.4 (lowest), which pushed real quick notes below random `*notes*` files. */
  'quick-notes': 0.68,
  extensions: 0.68,
  store: 0.25,
  'mac-cli': 0.46,
  'native-command': 0.7,
  commands: 0.66,
  snippets: 0.58,
  'quick-links': 0.55,
  calculator: 0.9,
  'color-converter': 0.9,
}

function normalizeRecency(ms: number): number {
  if (ms <= 0) return 0
  const oneDay = 24 * 60 * 60 * 1000
  const ageDays = ms / oneDay
  return 1 / (1 + ageDays)
}

function normalizeFrequency(frequency: number): number {
  if (frequency <= 0) return 0
  return Math.min(1, Math.log10(frequency + 1) / 2)
}

function fuzzyBonus(distance: number | undefined): number {
  if (distance === undefined) return 0
  if (distance <= 0) return 0.08
  if (distance === 1) return 0.05
  if (distance === 2) return 0.02
  return 0
}

export function computeWeightedScore(input: RankFeatures): number {
  const lexical = Math.max(0, Math.min(1, input.lexical))
  const recency = normalizeRecency(input.recencyMs)
  const frequency = normalizeFrequency(input.frequency)
  const success = Math.max(0, Math.min(1, input.successRate))
  const prior = CATEGORY_PRIOR[input.category] ?? 0.35
  const fuzzy = fuzzyBonus(input.fuzzyDistance)

  const popularity = input.popularity ? Math.min(1, Math.log10(input.popularity + 1) / 7) : 0
  const weighted =
    lexical * 0.6 +
    recency * 0.1 +
    frequency * 0.1 +
    success * 0.05 +
    prior * 0.05 +
    fuzzy +
    popularity * 0.1
  return Math.round(weighted * 1000)
}

export function shouldPreferRecent(leftScore: number, leftAgeMs: number, rightScore: number, rightAgeMs: number): boolean {
  const gap = Math.abs(leftScore - rightScore)
  if (gap > 20) return false
  return leftAgeMs < rightAgeMs
}
