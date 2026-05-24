import { listNativeCommands } from '../../nativeCommands/registry'
import type { IndexedDocument, SearchProvider } from './types'

/** Build indexed documents for every native command in the typed registry.
 *  The registry replaces the previous ad-hoc shell strings and the
 *  fragile mac-cli README scraper with a first-class catalog. */
function buildNativeCommandDocuments(): IndexedDocument[] {
  const now = Date.now()
  return listNativeCommands()
    .filter((descriptor) => descriptor.id !== 'list-listening-ports')
    .map((descriptor) => ({
      id: `native:${descriptor.id}`,
      category: 'native-command' as const,
      title: descriptor.title,
      subtitle: descriptor.subtitle,
      tokens: [descriptor.title, descriptor.subtitle, descriptor.category, ...descriptor.keywords].join(' '),
      action: { type: 'run-native-command' as const, commandId: descriptor.id },
      updatedAt: now,
    }))
}

function buildRaymesSurfaceDocuments(): IndexedDocument[] {
  const now = Date.now()
  return [
    {
      id: 'command:open-providers',
      title: 'Open Providers',
      subtitle: 'Raymes settings',
      keywords: ['providers', 'provider', '/providers'],
      commandId: 'open-providers',
    },
    {
      id: 'command:open-settings',
      title: 'Open Settings',
      subtitle: 'Raymes settings',
      keywords: ['settings', 'preferences', '/settings'],
      commandId: 'open-settings',
    },
    {
      id: 'command:open-extensions',
      title: 'Open Extensions',
      subtitle: 'Raymes extensions',
      keywords: ['extensions', 'raycast', '/extensions'],
      commandId: 'open-extensions',
    },
    {
      id: 'command:open-snippets',
      title: 'Open Snippets',
      subtitle: 'Raymes snippets',
      keywords: ['snippets', 'text snippets', '/snippets'],
      commandId: 'open-snippets',
    },
    {
      id: 'command:open-notes',
      title: 'Open Notes',
      subtitle: 'Raymes quick notes',
      keywords: ['notes', 'quick notes', '/notes'],
      commandId: 'open-notes',
    },
    {
      id: 'command:open-emoji-picker',
      title: 'Open Emoji Picker',
      subtitle: 'Raymes emoji picker',
      keywords: ['emoji', 'symbols', '/emoji'],
      commandId: 'open-emoji-picker',
    },
    {
      id: 'command:quit-raymes',
      title: 'Quit Raymes',
      subtitle: 'Close Raymes',
      keywords: ['quit', 'exit', '/quit'],
      commandId: 'quit-raymes',
    },
  ].map((item) => ({
    id: item.id,
    category: 'commands' as const,
    title: item.title,
    subtitle: item.subtitle,
    tokens: [item.title, item.subtitle, ...item.keywords].join(' '),
    action: { type: 'invoke-command' as const, commandId: item.commandId },
    updatedAt: now,
  }))
}

export const commandsProvider: SearchProvider = {
  providerId: 'commands',
  async buildDocuments(): Promise<IndexedDocument[]> {
    return [...buildRaymesSurfaceDocuments(), ...buildNativeCommandDocuments()]
  },
}
