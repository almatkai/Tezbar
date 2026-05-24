import {
  listInstalledRegistryExtensions,
} from '../../extension-registry'
import type { IndexedDocument, SearchProvider } from './types'

export const extensionsProvider: SearchProvider = {
  providerId: 'extensions',
  async buildDocuments(): Promise<IndexedDocument[]> {
    const installed = listInstalledRegistryExtensions()
    if (installed.length === 0) return []

    const out: IndexedDocument[] = []
    for (const ext of installed.slice(0, 100)) {
      for (const cmd of ext.commands) {
        out.push({
          id: `extcmd:${ext.id}:${cmd.name}`,
          category: 'extensions',
          title: cmd.title,
          subtitle: ext.name,
          tokens: `${cmd.title} ${cmd.name} ${ext.name} ${ext.slug} ${ext.id} ${ext.description || ''}`,
          action: {
            type: 'run-extension-command',
            extensionId: ext.id,
            commandName: cmd.name,
            title: cmd.title,
            commandArgumentDefinitions: cmd.argumentDefinitions,
          },
          updatedAt: ext.installedAt,
          popularity: ext.downloadCount || 0,
        })
      }
    }

    return out
  },
}
