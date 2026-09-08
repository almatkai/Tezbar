import { afterEach, describe, expect, it, vi } from 'vitest'
import { SearchIndexDatabase } from './indexDb'

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers() })

describe('search result cache', () => {
  it('evicts old queries while retaining recently used queries', () => {
    const index = new SearchIndexDatabase()
    const search = vi.spyOn(index, 'search').mockReturnValue([])
    for (let i = 0; i < 64; i++) index.getSearch(`query-${i}`, 80)
    index.getSearch('query-0', 80)
    index.getSearch('query-64', 80)
    search.mockClear()
    index.getSearch('query-0', 80)
    expect(search).not.toHaveBeenCalled()
    index.getSearch('query-1', 80)
    expect(search).toHaveBeenCalledOnce()
  })

  it('refreshes expired results and clears results on index changes', () => {
    vi.useFakeTimers()
    vi.setSystemTime(0)
    const index = new SearchIndexDatabase()
    const search = vi.spyOn(index, 'search').mockReturnValue([])
    index.getSearch('query', 80)
    index.getSearch('query', 80)
    expect(search).toHaveBeenCalledOnce()
    vi.setSystemTime(5 * 60 * 1000)
    index.getSearch('query', 80)
    expect(search).toHaveBeenCalledTimes(2)
    index.clearSearchCache()
    index.getSearch('query', 80)
    expect(search).toHaveBeenCalledTimes(3)
  })
})
