import { describe, expect, it } from 'vitest'

import {
  extensionCatalogReducer,
  INITIAL_EXTENSION_CATALOG_STATE,
  normalizeInstalledExtension,
} from './extensionCatalogState'
import { mergeCatalog } from './extensionRuntimeStatus'

describe('extension catalog state', () => {
  it('retains bundled artwork for installed entries absent from the current store page', () => {
    const artwork = '/extensions/artwork/perplexity.webp'
    const installed = normalizeInstalledExtension({
      id: 'com.tezbar.native.perplexity', slug: 'perplexity', name: 'Perplexity',
      description: '', author: 'Tezbar', version: '2', installedAt: 42,
      packageJsonPath: '/fixture/package.json', extensionPath: '/fixture', commands: [],
      iconPath: '/old/icon.png', iconUrl: artwork, artwork, artworkBackground: '#6B2C30',
    })
    const rows = mergeCatalog([], [installed], 'macOS')
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ iconUrl: artwork, artwork, artworkBackground: '#6B2C30' })
  })

  it('keeps legacy installed filesystem icons when artwork is absent', () => {
    const installed = normalizeInstalledExtension({
      id: 'raycast.example', slug: 'example', name: 'Example', description: '',
      version: '1', installedAt: 42, commands: [],
      packageJsonPath: '/fixture/package.json', extensionPath: '/fixture',
      iconPath: '/fixture/assets/icon.png',
    })
    expect(installed.iconUrl).toBe('/fixture/assets/icon.png')
    expect(installed.artwork).toBeUndefined()
  })

  it('retains installed runtime, Native V2 and platform metadata on load', () => {
    const started = extensionCatalogReducer(INITIAL_EXTENSION_CATALOG_STATE, {
      type: 'load-started', requestId: 9,
    })
    const metadata = {
      schemaVersion: 2 as const,
      id: 'com.tezbar.native.foo', version: '2.0', replaces: ['raycast.foo'],
      commands: [{ name: 'open', runtime: 'rust' as const, platforms: ['macOS' as const] }],
      totalCommands: 1, status: 'complete' as const, verification: [],
    }
    const loaded = extensionCatalogReducer(started, {
      type: 'load-succeeded', requestId: 9, store: [], installed: [{
        id: 'raycast.foo', name: 'Foo', description: '', author: 'Tezbar', version: '2.0',
        installedAt: 1, runtime: 'rust', nativeV2: metadata, platforms: ['macOS'],
      }],
    })
    expect(loaded.installed[0]).toMatchObject({
      runtime: 'rust', nativeV2: metadata, platforms: ['macOS'],
    })
  })

  it('commits load results and clears loading atomically', () => {
    const loading = extensionCatalogReducer(INITIAL_EXTENSION_CATALOG_STATE, {
      type: 'load-started',
      requestId: 1,
    })
    const loaded = extensionCatalogReducer(loading, {
      type: 'load-succeeded',
      requestId: 1,
      installed: [],
      store: [{ id: 'raycast.test', name: 'Test', description: '', author: '', version: '1' }],
    })
    expect(loaded.loading).toBe(false)
    expect(loaded.store).toHaveLength(1)
  })

  it('ignores stale load results from older search requests', () => {
    const firstLoading = extensionCatalogReducer(INITIAL_EXTENSION_CATALOG_STATE, {
      type: 'load-started',
      requestId: 1,
    })
    const latestLoading = extensionCatalogReducer(firstLoading, {
      type: 'load-started',
      requestId: 2,
    })
    const loaded = extensionCatalogReducer(latestLoading, {
      type: 'load-succeeded',
      requestId: 2,
      installed: [],
      store: [{ id: 'raycast.arc', name: 'Arc', description: '', author: '', version: '1' }],
    })
    const stale = extensionCatalogReducer(loaded, {
      type: 'load-succeeded',
      requestId: 1,
      installed: [],
      store: [{ id: 'raycast.brew', name: 'Brew', description: '', author: '', version: '1' }],
    })

    expect(stale.store.map((item) => item.name)).toEqual(['Arc'])
  })

  it('combines selection mode and install completion updates', () => {
    const selected = extensionCatalogReducer(INITIAL_EXTENSION_CATALOG_STATE, {
      type: 'selected',
      id: 'raycast.test',
      follow: false,
    })
    const installing = extensionCatalogReducer(selected, {
      type: 'install-started',
      id: 'raycast.test',
    })
    const finished = extensionCatalogReducer(installing, {
      type: 'install-finished',
      id: 'raycast.test',
      message: { tone: 'success', text: 'Installed Test' },
    })
    expect(finished.selectedId).toBe('raycast.test')
    expect(finished.followSelection).toBe(false)
    expect(finished.installing).toEqual({})
    expect(finished.message?.text).toBe('Installed Test')
  })

  it('removes completed progress entries', () => {
    const state = { ...INITIAL_EXTENSION_CATALOG_STATE, installing: { 'raycast.test': 52 } }
    expect(extensionCatalogReducer(state, {
      type: 'install-progress',
      id: 'raycast.test',
      progress: 100,
    }).installing).toEqual({})
  })

  it('supports pagination: load-more appends items and manages hasMore/total', () => {
    const loading = extensionCatalogReducer(INITIAL_EXTENSION_CATALOG_STATE, {
      type: 'load-started',
      requestId: 1,
    })
    const initial = extensionCatalogReducer(loading, {
      type: 'load-succeeded',
      requestId: 1,
      installed: [],
      store: [{ id: 'raycast.1', name: 'One', description: '', author: '', version: '1' }],
      total: 50,
      hasMore: true,
    })
    expect(initial.total).toBe(50)
    expect(initial.hasMore).toBe(true)

    const loadingMore = extensionCatalogReducer(initial, { type: 'load-more-started' })
    expect(loadingMore.loadingMore).toBe(true)

    const moreLoaded = extensionCatalogReducer(loadingMore, {
      type: 'load-more-succeeded',
      items: [{ id: 'raycast.2', name: 'Two', description: '', author: '', version: '1' }],
      total: 50,
      hasMore: true,
      selectNext: true,
    })
    expect(moreLoaded.loadingMore).toBe(false)
    expect(moreLoaded.store).toHaveLength(2)
    expect(moreLoaded.selectedId).toBe('raycast.2')
    expect(moreLoaded.followSelection).toBe(true)

    const failed = extensionCatalogReducer(loadingMore, {
      type: 'load-more-failed',
      message: 'Network error',
    })
    expect(failed.loadingMore).toBe(false)
    expect(failed.message?.text).toBe('Network error')
  })
})
