import { RAYMES_AI_NEW_CHAT_EVENT, RAYMES_QUICK_NOTE_SHORTCUT_EVENT } from '../shared/aiChatSurface'
import { RAYMES_NEW_SNIPPET_EVENT } from '../shared/snippetEvents'
import { TEZBAR_NEW_VAULT_ITEM_EVENT } from '../shared/vault'

/** The window event ⌘N / Ctrl+N dispatches on each surface. */
export function newItemEventFor(surface: string): string {
  if (surface === 'ai-chat') return RAYMES_AI_NEW_CHAT_EVENT
  if (surface === 'snippets') return RAYMES_NEW_SNIPPET_EVENT
  if (surface === 'vault') return TEZBAR_NEW_VAULT_ITEM_EVENT
  return RAYMES_QUICK_NOTE_SHORTCUT_EVENT
}
