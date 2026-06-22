import { describe, expect, it } from 'vitest'
import { rankDirectoryRecommendations } from './directoryRecommendations'

const NOW = Date.UTC(2026, 5, 20)

describe('rankDirectoryRecommendations', () => {
  it('ranks frequently visited directories first', () => {
    const ranked = rankDirectoryRecommendations(
      {
        '/work/occasional': { count: 2, lastVisitedAt: NOW },
        '/personal/frequent': { count: 8, lastVisitedAt: NOW - 86_400_000 },
      },
      { now: NOW },
    )

    expect(ranked.map((item) => item.path)).toEqual(['/personal/frequent', '/work/occasional'])
  })

  it('collapses several visited siblings into their parent', () => {
    const ranked = rankDirectoryRecommendations(
      {
        '/Users/dev/code/aml': { count: 4, lastVisitedAt: NOW - 3_000 },
        '/Users/dev/code/raymes': { count: 7, lastVisitedAt: NOW - 2_000 },
        '/Users/dev/code/site': { count: 2, lastVisitedAt: NOW - 1_000 },
        '/Users/dev/Documents': { count: 5, lastVisitedAt: NOW },
      },
      { now: NOW },
    )

    expect(ranked.map((item) => item.path)).toEqual([
      '/Users/dev/code',
      '/Users/dev/Documents',
    ])
    expect(ranked[0]?.count).toBe(13)
  })

  it('does not collapse into an excluded parent', () => {
    const ranked = rankDirectoryRecommendations(
      {
        '/Users/dev/Desktop': { count: 4, lastVisitedAt: NOW },
        '/Users/dev/Documents': { count: 3, lastVisitedAt: NOW },
        '/Users/dev/Downloads': { count: 2, lastVisitedAt: NOW },
      },
      { now: NOW, excludedPaths: ['/Users/dev'] },
    )

    expect(ranked.map((item) => item.path)).toEqual([
      '/Users/dev/Desktop',
      '/Users/dev/Documents',
      '/Users/dev/Downloads',
    ])
  })

  it('keeps the strongest concrete directory in a traversed branch', () => {
    const ranked = rankDirectoryRecommendations(
      {
        '/Users/dev/Desktop': { count: 6, lastVisitedAt: NOW - 3_000 },
        '/Users/dev/Desktop/code': { count: 6, lastVisitedAt: NOW - 2_000 },
        '/Users/dev/Desktop/code/raymes': { count: 3, lastVisitedAt: NOW - 1_000 },
        '/Users/dev/Documents': { count: 4, lastVisitedAt: NOW },
      },
      { now: NOW, excludedPaths: ['/Users/dev'] },
    )

    expect(ranked.map((item) => item.path)).toEqual([
      '/Users/dev/Desktop/code',
      '/Users/dev/Documents',
    ])
    expect(ranked[0]?.count).toBe(6)
  })
})
