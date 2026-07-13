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

  it('decisively promotes an item used three times inside the hot window', () => {
    expect(computeHotUsageBoost(2)).toBeLessThan(1000)
    expect(computeHotUsageBoost(3)).toBeGreaterThan(2500)
    expect(computeHotUsageBoost(6)).toBeGreaterThan(computeHotUsageBoost(3))
  })
})
