import { describe, expect, it } from 'vitest'

import {
  LAUNCHER_QUERY_HISTORY_LIMIT,
  addLauncherQueryHistoryEntry,
  launcherQueryHistoryEntry,
  parseLauncherQueryHistory,
  shouldRecallLastLauncherQuery,
} from './launcherQueryHistory'

describe('launcher query history', () => {
  it('records a submitted launcher query without surrounding whitespace', () => {
    expect(launcherQueryHistoryEntry('clipboard history ', false)).toBe('clipboard history')
  })

  it('preserves AI mode when normalizing a recalled prompt', () => {
    expect(launcherQueryHistoryEntry('explain this  ', false)).toBe(' explain this')
    expect(launcherQueryHistoryEntry(' explain this', false)).toBe(' explain this')
  })

  it('leaves terminal commands to terminal history', () => {
    expect(launcherQueryHistoryEntry('git status', true)).toBeNull()
  })

  it('keeps only the latest query', () => {
    expect(LAUNCHER_QUERY_HISTORY_LIMIT).toBe(1)
    expect(addLauncherQueryHistoryEntry('new query')).toEqual(['new query'])
  })

  it('only recalls the last query after ArrowUp reaches the top search result', () => {
    const keyContext = {
      isDeepSearchMode: false,
      key: 'ArrowUp',
      value: '',
      visibleResultCount: 10,
    }

    expect(shouldRecallLastLauncherQuery({ ...keyContext, selectedResultIndex: 8 })).toBe(false)
    expect(shouldRecallLastLauncherQuery({ ...keyContext, selectedResultIndex: 1 })).toBe(false)
    expect(shouldRecallLastLauncherQuery({ ...keyContext, selectedResultIndex: 0 })).toBe(true)
  })

  it('leaves ArrowDown for result navigation', () => {
    expect(
      shouldRecallLastLauncherQuery({
        isDeepSearchMode: false,
        key: 'ArrowDown',
        selectedResultIndex: 0,
        value: '',
        visibleResultCount: 10,
      })
    ).toBe(false)
  })

  it('never recalls a launcher query from Deep Search mode', () => {
    expect(
      shouldRecallLastLauncherQuery({
        isDeepSearchMode: true,
        key: 'ArrowUp',
        selectedResultIndex: -1,
        value: '!passwords',
        visibleResultCount: 10,
      })
    ).toBe(false)
  })

  it('safely parses persisted history', () => {
    expect(parseLauncherQueryHistory('["settings", 42, "", " clipboard"]')).toEqual(['settings'])
    expect(parseLauncherQueryHistory('not json')).toEqual([])
  })
})
