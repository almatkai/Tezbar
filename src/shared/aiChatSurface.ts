/**
 * How the AI chat surface was opened from the command bar (or global ⌘N).
 * Drives first-frame behaviour: submit a prompt, browse history, or force
 * a fresh session.
 */
export type AiChatBoot =
  | { kind: 'submit'; prompt: string; workingDirectory?: string; nonce?: string }
  | { kind: 'panel'; nonce?: string }
  | { kind: 'newChat'; workingDirectory?: string; nonce?: string }
  | { kind: 'screen'; nonce?: string }
  | { kind: 'resume'; sessionId: string; nonce?: string }

/** Give every launcher invocation an identity, including repeated commands. */
export function createAiChatNavigation(boot: AiChatBoot): AiChatBoot {
  return { ...boot, nonce: crypto.randomUUID() }
}

/** Dispatched from App when ⌘N fires on the command surface (quick note). */
export const RAYMES_QUICK_NOTE_SHORTCUT_EVENT = 'tezbar:quick-note-shortcut'

/** Dispatched from App when ⌘N should start a new chat (AI chat surface). */
export const RAYMES_AI_NEW_CHAT_EVENT = 'tezbar:ai-new-chat'
