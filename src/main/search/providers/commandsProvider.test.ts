import { describe, expect, it } from 'vitest'
import { commandsProvider } from './commandsProvider'

describe('commandsProvider', () => {
  it('indexes only the Tezbar surface command for quick notes', async () => {
    const documents = await commandsProvider.buildDocuments()
    const quickNotesDocuments = documents.filter((document) =>
      document.action.type === 'invoke-command'
        ? document.action.commandId === 'open-notes'
        : document.action.type === 'run-native-command' &&
          document.action.commandId === 'open-quick-notes'
    )

    expect(quickNotesDocuments).toEqual([
      expect.objectContaining({
        id: 'command:open-notes',
        title: 'Open Notes',
        subtitle: 'Tezbar quick notes',
        category: 'commands',
        action: { type: 'invoke-command', commandId: 'open-notes' },
      }),
    ])
  })

  it('indexes only the Tezbar surface command for the emoji picker', async () => {
    const documents = await commandsProvider.buildDocuments()
    const emojiPickerDocuments = documents.filter((document) =>
      document.action.type === 'invoke-command'
        ? document.action.commandId === 'open-emoji-picker'
        : document.action.type === 'run-native-command' &&
          document.action.commandId === 'open-emoji-picker'
    )

    expect(emojiPickerDocuments).toEqual([
      expect.objectContaining({
        id: 'command:open-emoji-picker',
        title: 'Open Emoji Picker',
        subtitle: 'Tezbar emoji picker',
        category: 'commands',
        action: { type: 'invoke-command', commandId: 'open-emoji-picker' },
      }),
    ])
  })
})
