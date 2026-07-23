import { describe, expect, it } from 'vitest'
import { selectVolatileSearchProviders } from './providerRefreshPolicy'

describe('selectVolatileSearchProviders', () => {
  it('keeps expensive extension discovery out of interactive refreshes', () => {
    const providers = [
      'commands',
      'clipboard',
      'notes',
      'snippets',
      'quick-links',
      'apps',
      'extensions',
      'files',
    ].map((providerId) => ({ providerId }))

    expect(selectVolatileSearchProviders(providers).map((provider) => provider.providerId)).toEqual(
      ['commands', 'clipboard', 'notes', 'snippets', 'quick-links']
    )
  })
})
