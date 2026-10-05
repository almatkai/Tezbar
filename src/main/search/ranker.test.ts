import { describe, expect, it } from 'vitest'
import {
  computeHotUsageBoost,
  computeLearnedUsageBoost,
  computeQueryLearningBoost,
  computeWeightedScore,
} from './ranker'

describe('search ranker learned usage', () => {
  it('lets a frequently used matching native command outrank an unused exact app hit', () => {
    const now = Date.now()
    const unusedExactAppScore =
      computeWeightedScore({
        lexical: 1,
        recencyMs: 0,
        frequency: 0,
        successRate: 0,
        category: 'applications',
      }) + 600

    const usedNativeCommandScore =
      computeWeightedScore({
        lexical: 0.75,
        recencyMs: 0,
        frequency: 4,
        successRate: 1,
        category: 'native-command',
      }) +
      300 +
      computeLearnedUsageBoost({
        category: 'native-command',
        frequency: 4,
        successRate: 1,
        lastUsedAt: now,
        now,
      })

    expect(usedNativeCommandScore).toBeGreaterThan(unusedExactAppScore)
  })

  it('does not boost unvisited commands', () => {
    expect(
      computeLearnedUsageBoost({
        category: 'native-command',
        frequency: 0,
        successRate: 0,
        lastUsedAt: 0,
        now: Date.now(),
      })
    ).toBe(0)
  })

  it('lets a query-specific pick beat a literal match on the next search', () => {
    const now = Date.now()
    const literalMatchScore =
      computeWeightedScore({
        lexical: 1,
        recencyMs: 0,
        frequency: 0,
        successRate: 0,
        category: 'applications',
      }) + 600

    const learnedFuzzyPickScore =
      computeWeightedScore({
        lexical: 0.58,
        recencyMs: 0,
        frequency: 0,
        successRate: 0,
        category: 'native-command',
      }) +
      computeQueryLearningBoost({
        frequency: 1,
        successRate: 1,
        lastUsedAt: now,
        now,
      })

    expect(learnedFuzzyPickScore).toBeGreaterThan(literalMatchScore)
  })

  it('graduates recency boost from 5min, 30min, 1h, 24h, 7d, to 30d', () => {
    const now = 1_000_000_000_000
    const oneMin = 60 * 1000
    const oneHour = 60 * oneMin
    const oneDay = 24 * oneHour

    // frequency = 1 (log2(2)*220 = 220), successRate = 0 (successBoost = 0)
    const baseFreq = 220

    // < 5 min -> recency = 500
    expect(
      computeLearnedUsageBoost({
        category: 'extensions',
        frequency: 1,
        successRate: 0,
        lastUsedAt: now - 2 * oneMin,
        now,
      })
    ).toBe(500 + baseFreq)

    // < 30 min -> recency = 450
    expect(
      computeLearnedUsageBoost({
        category: 'extensions',
        frequency: 1,
        successRate: 0,
        lastUsedAt: now - 15 * oneMin,
        now,
      })
    ).toBe(450 + baseFreq)

    // < 1 hour -> recency = 400
    expect(
      computeLearnedUsageBoost({
        category: 'extensions',
        frequency: 1,
        successRate: 0,
        lastUsedAt: now - 45 * oneMin,
        now,
      })
    ).toBe(400 + baseFreq)

    // < 24 hours -> recency = 360
    expect(
      computeLearnedUsageBoost({
        category: 'extensions',
        frequency: 1,
        successRate: 0,
        lastUsedAt: now - 5 * oneHour,
        now,
      })
    ).toBe(360 + baseFreq)

    // < 7 days -> recency = 220
    expect(
      computeLearnedUsageBoost({
        category: 'extensions',
        frequency: 1,
        successRate: 0,
        lastUsedAt: now - 3 * oneDay,
        now,
      })
    ).toBe(220 + baseFreq)

    // < 30 days -> recency = 100
    expect(
      computeLearnedUsageBoost({
        category: 'extensions',
        frequency: 1,
        successRate: 0,
        lastUsedAt: now - 15 * oneDay,
        now,
      })
    ).toBe(100 + baseFreq)

    // >= 30 days -> recency = 0
    expect(
      computeLearnedUsageBoost({
        category: 'extensions',
        frequency: 1,
        successRate: 0,
        lastUsedAt: now - 35 * oneDay,
        now,
      })
    ).toBe(0 + baseFreq)
  })

  it('decisively promotes an item used three times inside the hot window', () => {
    expect(computeHotUsageBoost(2)).toBeLessThan(1000)
    expect(computeHotUsageBoost(3)).toBeGreaterThan(2500)
    expect(computeHotUsageBoost(6)).toBeGreaterThan(computeHotUsageBoost(3))
  })
})
