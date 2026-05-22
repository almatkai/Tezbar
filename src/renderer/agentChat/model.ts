import {
  CHAT_CONTEXT_MAX_TURNS,
  type ChatSession,
} from '../../shared/chat'

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

export function buildAgentPromptFromChat(session: ChatSession, nextUserText: string): string {
  const turns = session.turns.slice()
  const lastTurn = turns.at(-1)
  if (lastTurn?.role === 'user' && lastTurn.text.trim() === nextUserText.trim()) {
    turns.pop()
  }
  const priorTurns = turns.slice(-CHAT_CONTEXT_MAX_TURNS)
  const lines: string[] = [
    'You are running inside Raymes on the user machine through the Pi agent harness.',
    'You have tool access. You can run bash commands and inspect local files/folders/apps on this Mac.',
    'For local/system questions, actively use bash/read/listing tools before answering.',
    'Never say you cannot access the computer when the requested information can be inspected with bash.',
    'When the user asks what is installed or what they have, inspect /Applications, ~/Applications, PATH, or relevant local locations.',
    'Resolve common macOS shorthand like "desktop/code" to ~/Desktop/code when appropriate.',
  ]
  if (priorTurns.length === 0) {
    lines.push('', 'User message:', nextUserText)
    return lines.join('\n')
  }
  lines.push('', 'Prior conversation (for context only):')
  for (const turn of priorTurns) {
    const label = turn.role === 'user' ? 'User' : 'Assistant'
    lines.push(`${label}: ${turn.text}`.trim())
  }
  lines.push('', 'New message from the user:', nextUserText)
  return lines.join('\n\n')
}
