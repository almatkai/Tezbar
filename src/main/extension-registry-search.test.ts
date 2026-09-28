import { describe, expect, it } from 'vitest'

import { scoreCatalogEntrySearch, searchExtensionCatalog, type CatalogEntry } from './extension-registry'

function catalogEntry(overrides: Partial<CatalogEntry>): CatalogEntry {
  return {
    name: 'example',
    title: 'Example',
    description: '',
    author: '',
    contributors: [],
    icon: '',
    iconUrl: '',
    screenshotUrls: [],
    categories: [],
    platforms: [],
    commands: [],
    ...overrides,
  }
}

describe('extension catalog search scoring', () => {
  it('does not match short queries in the middle of unrelated words', () => {
    const brew = catalogEntry({
      name: 'brew',
      title: 'Brew',
      description: 'Search and install Homebrew packages',
    })
    const arc = catalogEntry({
      name: 'arc',
      title: 'Arc',
      description: 'Control Arc browser tabs',
    })

    expect(scoreCatalogEntrySearch(brew, 'arc')).toBe(0)
    expect(scoreCatalogEntrySearch(arc, 'arc')).toBeGreaterThan(0)
  })

  it('still allows word-prefix matches for short queries', () => {
    const archive = catalogEntry({
      name: 'archive-tabs',
      title: 'Archive Tabs',
      description: 'Archive browser tabs',
    })

    expect(scoreCatalogEntrySearch(archive, 'arc')).toBeGreaterThan(0)
  })

  it('paginates extension catalog results with offset, limit, total, and hasMore', async () => {
    const page1 = await searchExtensionCatalog('', { offset: 0, limit: 25 })
    expect(page1.items.length).toBeLessThanOrEqual(25)
    expect(page1.limit).toBe(25)
    expect(page1.offset).toBe(0)
    expect(page1.total).toBeGreaterThan(0)
    if (page1.total > 25) {
      expect(page1.hasMore).toBe(true)
      expect(page1.items.length).toBe(25)

      const page2 = await searchExtensionCatalog('', { offset: 25, limit: 25 })
      expect(page2.offset).toBe(25)
      expect(page2.items.length).toBeLessThanOrEqual(25)
      // Items on page 2 should be different from page 1
      expect(page2.items[0]?.id).not.toBe(page1.items[0]?.id)
    }
  })
})
