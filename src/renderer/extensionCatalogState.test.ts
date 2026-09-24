import { describe, expect, it } from 'vitest'

import {
  extensionCatalogReducer,
  INITIAL_EXTENSION_CATALOG_STATE,
} from './extensionCatalogState'

describe('extension catalog state', () => {
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
