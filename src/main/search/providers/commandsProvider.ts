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

export const commandsProvider: SearchProvider = {
  providerId: 'commands',
  async buildDocuments(): Promise<IndexedDocument[]> {
    return buildNativeCommandDocuments()
  },
}
