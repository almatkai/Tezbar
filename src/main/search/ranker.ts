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

type LearnedUsageFeatures = {
  category: SearchCategory
  frequency: number
  successRate: number
  lastUsedAt: number
  now?: number
}

type QueryLearningFeatures = {
  frequency: number
  successRate: number
  lastUsedAt: number
  now?: number
}

const CATEGORY_PRIOR: Record<SearchCategory, number> = {
  applications: 0.72,
  files: 0.6,
  knowledge: 0.64,
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

function isLearnedUsageCategory(category: SearchCategory): boolean {
  return (
    category === 'applications' ||
    category === 'extensions' ||
    category === 'native-command' ||
    category === 'commands' ||
    category === 'quick-notes' ||
    category === 'snippets' ||
    category === 'quick-links'
  )
}

export function computeLearnedUsageBoost(input: LearnedUsageFeatures): number {
  if (!isLearnedUsageCategory(input.category) || input.frequency <= 0 || input.lastUsedAt <= 0) {
    return 0
  }

  const now = input.now ?? Date.now()
  const ageMs = Math.max(0, now - input.lastUsedAt)
  const oneDay = 24 * 60 * 60 * 1000
  const recencyBoost =
    ageMs < oneDay ? 360 : ageMs < 7 * oneDay ? 220 : ageMs < 30 * oneDay ? 100 : 0
  const frequencyBoost = Math.min(900, Math.log2(input.frequency + 1) * 220)
  const successBoost = input.successRate >= 0.5 ? 120 : 0

  return Math.round(recencyBoost + frequencyBoost + successBoost)
}

export function computeQueryLearningBoost(input: QueryLearningFeatures): number {
  if (input.frequency <= 0 || input.lastUsedAt <= 0) return 0

  const now = input.now ?? Date.now()
  const ageMs = Math.max(0, now - input.lastUsedAt)
  const oneDay = 24 * 60 * 60 * 1000
  const recencyBoost =
    ageMs < oneDay ? 1250 : ageMs < 7 * oneDay ? 900 : ageMs < 30 * oneDay ? 550 : 250
  const frequencyBoost = Math.min(1200, Math.log2(input.frequency + 1) * 350)
  const successBoost = input.successRate >= 0.5 ? 150 : 0

  return Math.round(recencyBoost + frequencyBoost + successBoost)
}

/** A short burst is stronger intent than lifetime frequency. Three successful
 * uses inside the five-minute window should dominate the home recommendations,
 * then decay naturally as those persisted events leave the window. */
export function computeHotUsageBoost(recentUseCount: number): number {
  if (recentUseCount <= 0) return 0
  if (recentUseCount === 1) return 180
  if (recentUseCount === 2) return 600
  return Math.min(3600, 2600 + (recentUseCount - 3) * 250)
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

export function shouldPreferRecent(
  leftScore: number,
  leftAgeMs: number,
  rightScore: number,
  rightAgeMs: number
): boolean {
  const gap = Math.abs(leftScore - rightScore)
  if (gap > 20) return false
  return leftAgeMs < rightAgeMs
}
