import { listInstalledRegistryExtensions } from '../../extension-registry'
import { getCommandAliases, getDisabledCommands } from '../../llm/configStore'
import type { IndexedDocument, SearchProvider } from './types'

export const extensionsProvider: SearchProvider = {
  providerId: 'extensions',
  async buildDocuments(): Promise<IndexedDocument[]> {
    const installed = listInstalledRegistryExtensions()
    if (installed.length === 0) return []

    const disabled = getDisabledCommands()
    const aliases = getCommandAliases()
    const out: IndexedDocument[] = []
    for (const ext of installed.slice(0, 100)) {
      for (const cmd of ext.commands) {
        const commandId = cmd.launcher?.id ?? `extcmd:${ext.id}:${cmd.name}`
        if (disabled[commandId]) continue

        let tokens = `${cmd.title} ${cmd.name} ${ext.name} ${ext.slug} ${ext.id} ${ext.description || ''}`
        const alias = aliases[commandId]
        if (alias) tokens += ` ${alias}`

        out.push({
          id: commandId,
          category: 'extensions',
          title: cmd.title,
          subtitle: ext.name,
          tokens,
          action: {
            type: 'run-extension-command',
            extensionId: ext.id,
            commandName: cmd.name,
            launcher: cmd.launcher,
            title: cmd.title,
            iconPath: ext.iconPath,
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
