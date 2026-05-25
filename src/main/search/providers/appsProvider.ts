import { readdirSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import type { IndexedDocument, SearchProvider } from './types'

export function listApplications(): Array<{ name: string; path: string }> {
  const roots = [
    '/Applications',
    '/Applications/Utilities',
    '/System/Applications',
    '/System/Applications/Utilities',
    '/System/Library/CoreServices/Applications',
    '/System/Library/CoreServices',
    join(homedir(), 'Applications'),
  ]
  const out: Array<{ name: string; path: string }> = []
  const seen = new Set<string>()

  for (const root of roots) {
    try {
      for (const entry of readdirSync(root)) {
        if (!entry.endsWith('.app')) continue
        const name = entry.replace(/\.app$/, '')
        if (seen.has(name)) continue
        seen.add(name)
        out.push({
          name,
          path: join(root, entry),
        })
      }
    } catch {
      // Ignore inaccessible roots.
    }
  }

  return out
}

export const appsProvider: SearchProvider = {
  providerId: 'apps',
  async buildDocuments(): Promise<IndexedDocument[]> {
    const now = Date.now()
    return listApplications().map((app) => ({
      id: `app:${app.path}`,
      category: 'applications',
      title: app.name,
      subtitle: app.path,
      tokens: `${app.name} ${app.path}`,
      action: { type: 'open-app', appName: app.name },
      updatedAt: now,
      sourcePath: app.path,
    }))
  },
}
