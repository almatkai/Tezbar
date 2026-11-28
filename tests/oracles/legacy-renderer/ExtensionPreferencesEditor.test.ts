import { describe, expect, it } from 'vitest'

import {
  mergePreferenceSetups,
  partitionPreferenceValues,
  preferenceValueKey,
  type PreferenceField,
} from './extensionPreferences'

describe('extension preference scopes', () => {
  const globalField: PreferenceField = { name: 'token', type: 'password' }
  const searchField: PreferenceField = {
    name: 'limit',
    commandName: 'search',
    commandTitle: 'Search',
  }
  const recentField: PreferenceField = {
    name: 'limit',
    commandName: 'recent',
    commandTitle: 'Recent',
  }

  it('keeps same-named command values isolated while merging setups', () => {
    const editor = mergePreferenceSetups(
      { preferences: [globalField], values: { token: 'secret' } },
      [
        { preferences: [globalField, searchField], values: { token: 'secret', limit: '10' } },
        { preferences: [globalField, recentField], values: { token: 'secret', limit: '25' } },
      ],
    )

    expect(editor.fields).toEqual([globalField, searchField, recentField])
    expect(editor.values).toEqual({
      '$extension:token': 'secret',
      'search:limit': '10',
      'recent:limit': '25',
    })
    expect(preferenceValueKey(searchField)).not.toBe(preferenceValueKey(recentField))
  })

  it('partitions editor values back into extension and command records', () => {
    expect(
      partitionPreferenceValues(
        [globalField, searchField, recentField],
        {
          '$extension:token': 'updated',
          'search:limit': '15',
          'recent:limit': '30',
        },
      ),
    ).toEqual([
      { commandName: undefined, values: { token: 'updated' } },
      { commandName: 'search', values: { limit: '15' } },
      { commandName: 'recent', values: { limit: '30' } },
    ])
  })
})
