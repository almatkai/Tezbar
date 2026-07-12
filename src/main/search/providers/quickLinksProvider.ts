import { app } from '@tezbar/desktop-runtime'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { IndexedDocument, SearchProvider } from './types'

type QuickLink = {
  id: string
  name: string
  template: string
  profile?: string
  createdAt: number
}

type QuickLinksDb = {
  links: QuickLink[]
}

function quickLinksPath(): string {
  const dir = join(app.getPath('userData'), 'search')
  mkdirSync(dir, { recursive: true })
  return join(dir, 'quick-links.json')
}

function readQuickLinksDb(): QuickLinksDb {
  try {
    const raw = readFileSync(quickLinksPath(), 'utf8')
    const parsed = JSON.parse(raw) as Partial<QuickLinksDb>
    if (!Array.isArray(parsed.links)) return { links: [] }
    return { links: parsed.links }
  } catch {
    const db: QuickLinksDb = {
      links: [
        {
          id: 'ql:google',
          name: 'Google Search',
          template: 'https://www.google.com/search?q={query}',
          createdAt: Date.now(),
        },
      ],
    }
    writeFileSync(quickLinksPath(), `${JSON.stringify(db, null, 2)}\n`, 'utf8')
    return db
  }
}

function fillTemplate(template: string, query: string): string {
  const q = encodeURIComponent(query.trim())
  return template.split('{query}').join(q)
}

export function resolveQuickLink(template: string, query: string): string {
  return fillTemplate(template, query)
}

export const quickLinksProvider: SearchProvider = {
  providerId: 'quick-links',
  async buildDocuments(): Promise<IndexedDocument[]> {
    return readQuickLinksDb().links.map((link) => ({
      id: link.id,
      category: 'quick-links',
      title: link.name,
      subtitle: link.profile ? `Quick link (${link.profile})` : 'Quick link',
      tokens: `${link.name} ${link.template}`,
      action: { type: 'open-url', url: fillTemplate(link.template, '') },
      updatedAt: link.createdAt,
    }))
  },
}
