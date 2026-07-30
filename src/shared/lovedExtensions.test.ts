import { describe, expect, it } from 'vitest'

import { LOVED_EXTENSIONS, searchLovedExtensions } from './lovedExtensions'

describe('loved extensions', () => {
  it('includes the Tezbar color picker repository', () => {
    expect(LOVED_EXTENSIONS).toContainEqual(
      expect.objectContaining({
        id: 'raycast.tezbar-color-picker',
        repository: 'https://github.com/almatkai/tezbar-color-picker-extension',
        iconUrl:
          'https://raw.githubusercontent.com/almatkai/tezbar-color-picker-extension/master/assets/icon.svg',
      })
    )
  })

  it('searches the curated list locally', () => {
    expect(searchLovedExtensions('pixel').map((extension) => extension.name)).toEqual([
      'Color Picker',
    ])
    expect(searchLovedExtensions('does-not-exist')).toEqual([])
  })
})
