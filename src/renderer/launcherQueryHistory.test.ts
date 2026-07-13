import { describe, expect, it } from 'vitest'

import {
  addLauncherQueryHistoryEntry,
  launcherQueryHistoryEntry,
  parseLauncherQueryHistory,
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

  it('keeps the latest unique query first', () => {
    expect(addLauncherQueryHistoryEntry(['settings', 'clipboard'], 'clipboard')).toEqual([
      'clipboard',
      'settings',
    ])
  })

  it('safely parses persisted history', () => {
    expect(parseLauncherQueryHistory('["settings", 42, "", " clipboard"]')).toEqual([
      'settings',
      ' clipboard',
    ])
    expect(parseLauncherQueryHistory('not json')).toEqual([])
  })
})
