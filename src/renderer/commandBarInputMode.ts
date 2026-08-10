import type { AiChatBoot } from '../shared/aiChatSurface'
import { parseSearchQuery } from '../shared/searchMode'

export type CommandBarInputMode = {
  parsedSearchQuery: ReturnType<typeof parseSearchQuery>
  isDeepSearchMode: boolean
  isAiMode: boolean
  aiTask: string
  aiWorkingDirectory?: string
  slashQuery: string
  isSlashInput: boolean
  isApplicationInput: boolean
  isCompletionInput: boolean
}

/** Derive launcher modes in one place so overlapping sentinels have an explicit priority. */
export function aiChatBootForInput(
  mode: Pick<CommandBarInputMode, 'aiTask' | 'aiWorkingDirectory'>,
  resolvedWorkingDirectory?: string
): AiChatBoot | null {
  if (mode.aiWorkingDirectory && resolvedWorkingDirectory && !mode.aiTask) {
    return { kind: 'newChat', workingDirectory: resolvedWorkingDirectory }
  }
  if (!mode.aiTask) return null
  return {
    kind: 'submit',
    prompt: mode.aiTask,
    workingDirectory: resolvedWorkingDirectory,
  }
}

export function commandBarInputMode(value: string, terminalMode: boolean): CommandBarInputMode {
  const parsedSearchQuery = parseSearchQuery(value)
  const isDeepSearchMode = !terminalMode && parsedSearchQuery.mode === 'deep'
  const aiSeparatorIndex = value.indexOf('  ')
  const isAiMode =
    !terminalMode && !isDeepSearchMode && (value.startsWith(' ') || aiSeparatorIndex >= 0)
  let aiTask = ''
  let aiWorkingDirectory: string | undefined
  if (isAiMode) {
    if (value.startsWith(' ')) {
      aiTask = value.trim()
    } else {
      const prefix = value.slice(0, aiSeparatorIndex).trim()
      const suffix = value.slice(aiSeparatorIndex + 2).trim()
      if (prefix.startsWith('/')) {
        aiWorkingDirectory = prefix
        aiTask = suffix
      } else {
        aiTask = `${prefix} ${suffix}`.trim()
      }
    }
  }
  const slashQuery = value.trimStart()
  const isSlashInput = !terminalMode && slashQuery.startsWith('/')
  const isApplicationInput = !terminalMode && slashQuery.startsWith('`')

  return {
    parsedSearchQuery,
    isDeepSearchMode,
    isAiMode,
    aiTask,
    aiWorkingDirectory,
    slashQuery,
    isSlashInput,
    isApplicationInput,
    isCompletionInput: !terminalMode && !isAiMode && (isSlashInput || isApplicationInput),
  }
}
