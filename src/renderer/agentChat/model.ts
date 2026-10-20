import { CHAT_CONTEXT_MAX_TURNS, type ChatSession } from '../../shared/chat'

/** Random id generator for chat sessions + turns. */
export function makeChatId(): string {
  return Math.random().toString(36).slice(2, 10) + Date.now().toString(36)
}

export function summarizeChatTitle(firstUserText: string): string {
  const firstLine = firstUserText.split('\n').find((l) => l.trim()) ?? ''
  const cleaned = firstLine.trim()
  if (!cleaned) return 'New chat'
  return cleaned.length > 64 ? cleaned.slice(0, 61) + '…' : cleaned
}

/** Placeholder for the history panel: an empty list while the backend is still
 *  answering is not the same as having no saved conversations. */
export function historyPlaceholder(loading: boolean): string {
  return loading ? 'Loading conversations…' : 'No saved conversations yet.'
}

export function buildAgentPromptFromChat(session: ChatSession, nextUserText: string): string {
  const turns = session.turns.slice()
  const lastTurn = turns.at(-1)
  if (lastTurn?.role === 'user' && lastTurn.text.trim() === nextUserText.trim()) {
    turns.pop()
  }
  const priorTurns = turns.slice(-CHAT_CONTEXT_MAX_TURNS)
  const lines: string[] = [
    'You are running inside Tezbar on the user machine through the Pi agent harness.',
    'You have tool access. You can run commands and inspect local files, folders, apps, and live system state on this computer.',
    'For performance diagnostics, gather live evidence with read-only commands before answering. Do not kill processes or change settings just to inspect them.',
    'Report only observed data, with units and context. Never invent CPU, memory, temperature, or tool results; clearly say when a metric is unavailable.',
    'Prefer process names over full command-line arguments in diagnostics, to avoid exposing credentials.',
    'Use launcher_search first to find local files, folders, applications, commands, clipboard items, notes, snippets, or links by name or metadata.',
    'Use pc_search first to find text or information inside indexed documents, PDFs, screenshots, images, and notes; use pc_read for nearby context.',
    'Never start with find, mdfind, grep, rg, or a recursive shell scan across the home, Desktop, Documents, Downloads, Pictures, or code folders. Tezbar indexed search must be tried first.',
    'For narrow codebase inspection and live system facts such as processes or ports, actively use bash/read/listing tools before answering.',
    'Never say you cannot access the computer when the requested information can be inspected with bash.',
    'When the user asks what is installed, inspect platform-appropriate locations, such as /Applications on macOS or Start Menu applications on Windows, and PATH.',
    'Resolve common macOS shorthand like "desktop/code" to ~/Desktop/code when appropriate.',
  ]
  if (session.workingDirectory) {
    lines.push(
      `Your working directory is ${JSON.stringify(session.workingDirectory)}.`,
      'Stay inside this directory unless the user explicitly asks you to inspect or change something outside it.'
    )
  }
  if (priorTurns.length === 0) {
    lines.push('', 'User message:', nextUserText)
    return lines.join('\n')
  }
  lines.push('', 'Prior conversation (for context only):')
  for (const turn of priorTurns) {
    const label = turn.role === 'user' ? 'User' : 'Assistant'
    const body = turn.error
      ? `${turn.text}\n[This attempt ended with an error: ${turn.error}]`
      : turn.text
    lines.push(`${label}: ${body}`.trim())
  }
  lines.push('', 'New message from the user:', nextUserText)
  return lines.join('\n\n')
}
