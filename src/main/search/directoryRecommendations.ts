import { dirname } from 'node:path'

export type DirectoryVisit = {
  count: number
  lastVisitedAt: number
}

export type DirectoryRecommendation = {
  path: string
  count: number
  lastVisitedAt: number
  score: number
}

type RankDirectoryRecommendationOptions = {
  now?: number
  limit?: number
  siblingThreshold?: number
  excludedPaths?: Iterable<string>
}

function visitScore(visit: DirectoryVisit, now: number): number {
  const ageDays = Math.max(0, (now - visit.lastVisitedAt) / 86_400_000)
  const recencyBoost = Math.max(0, 14 - ageDays)
  return visit.count * 10 + recencyBoost
}

export function rankDirectoryRecommendations(
  visits: Record<string, DirectoryVisit>,
  options: RankDirectoryRecommendationOptions = {},
): DirectoryRecommendation[] {
  const now = options.now ?? Date.now()
  const limit = options.limit ?? 5
  const siblingThreshold = options.siblingThreshold ?? 3
  const excluded = new Set(options.excludedPaths ?? [])
  const validVisits = Object.entries(visits).filter(
    ([path, visit]) =>
      path.startsWith('/') &&
      visit !== null &&
      typeof visit === 'object' &&
      Number.isFinite(visit.count) &&
      visit.count > 0 &&
      Number.isFinite(visit.lastVisitedAt),
  )

  const childrenByParent = new Map<string, Array<[string, DirectoryVisit]>>()
  for (const entry of validVisits) {
    const parent = dirname(entry[0])
    const siblings = childrenByParent.get(parent) ?? []
    siblings.push(entry)
    childrenByParent.set(parent, siblings)
  }

  const collapsedParents = new Set(
    Array.from(childrenByParent.entries())
      .filter(([parent, children]) => !excluded.has(parent) && children.length >= siblingThreshold)
      .map(([parent]) => parent),
  )
  const recommendations = new Map<string, DirectoryRecommendation>()

  for (const [path, visit] of validVisits) {
    const parent = dirname(path)
    const recommendationPath = collapsedParents.has(parent) ? parent : path

    if (excluded.has(recommendationPath)) continue

    const existing = recommendations.get(recommendationPath)
    const score = visitScore(visit, now)
    recommendations.set(recommendationPath, {
      path: recommendationPath,
      count: (existing?.count ?? 0) + visit.count,
      lastVisitedAt: Math.max(existing?.lastVisitedAt ?? 0, visit.lastVisitedAt),
      score: (existing?.score ?? 0) + score,
    })
  }

  const ranked = Array.from(recommendations.values())
    .sort(
      (a, b) =>
        b.score - a.score ||
        b.lastVisitedAt - a.lastVisitedAt ||
        a.path.localeCompare(b.path),
    )

  return ranked
    .filter((candidate, index) => {
      return !ranked.slice(0, index).some((stronger) => {
        return (
          candidate.path.startsWith(`${stronger.path}/`) ||
          stronger.path.startsWith(`${candidate.path}/`)
        )
      })
    })
    .slice(0, Math.max(0, limit))
}
