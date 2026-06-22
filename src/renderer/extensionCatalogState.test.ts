import { describe, expect, it } from 'vitest'

import {
  extensionCatalogReducer,
  INITIAL_EXTENSION_CATALOG_STATE,
} from './extensionCatalogState'

describe('extension catalog state', () => {
  it('commits load results and clears loading atomically', () => {
    const loading = extensionCatalogReducer(INITIAL_EXTENSION_CATALOG_STATE, {
      type: 'load-started',
    })
    const loaded = extensionCatalogReducer(loading, {
      type: 'load-succeeded',
      installed: [],
      store: [{ id: 'raycast.test', name: 'Test', description: '', author: '', version: '1' }],
    })
    expect(loaded.loading).toBe(false)
    expect(loaded.store).toHaveLength(1)
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
})
