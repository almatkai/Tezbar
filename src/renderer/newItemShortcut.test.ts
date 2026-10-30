import { describe, expect, it } from 'vitest'
import { RAYMES_AI_NEW_CHAT_EVENT, RAYMES_QUICK_NOTE_SHORTCUT_EVENT } from '../shared/aiChatSurface'
import { RAYMES_NEW_SNIPPET_EVENT } from '../shared/snippetEvents'
import { TEZBAR_NEW_VAULT_ITEM_EVENT } from '../shared/vault'
import { newItemEventFor } from './newItemShortcut'

describe('⌘N routing', () => {
  it('creates a vault secret while the Vault is open', () => {
    expect(newItemEventFor('vault')).toBe(TEZBAR_NEW_VAULT_ITEM_EVENT)
  })

  it('keeps the existing targets for other surfaces', () => {
    expect(newItemEventFor('ai-chat')).toBe(RAYMES_AI_NEW_CHAT_EVENT)
    expect(newItemEventFor('snippets')).toBe(RAYMES_NEW_SNIPPET_EVENT)
    expect(newItemEventFor('command')).toBe(RAYMES_QUICK_NOTE_SHORTCUT_EVENT)
    expect(newItemEventFor('clipboard')).toBe(RAYMES_QUICK_NOTE_SHORTCUT_EVENT)
  })
})
