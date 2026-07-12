import type { WebContents } from '@tezbar/desktop-runtime'
import type { Message } from './provider'
import { retrieveMemories, rememberMemory } from './memoryStore'
import { readLLMConfig, getProviderForTask } from './registry'

const HERMES_ANSWER_SYSTEM =
  'You are Hermes, a helpful assistant. Answer briefly and clearly unless the user asks for more detail.'

export async function streamAnswerToRenderer(
  sender: WebContents,
  userText: string,
  signal?: AbortSignal,
): Promise<void> {
  const token = (text: string): void => {
    if (!sender.isDestroyed()) sender.send('stream-token', text)
  }
  const done = (): void => {
    if (!sender.isDestroyed()) sender.send('stream-done')
  }
  const err = (message: string): void => {
    if (!sender.isDestroyed()) sender.send('stream-error', message)
  }

  const cfg = readLLMConfig()
  const provider = getProviderForTask('chat')
  const memories = retrieveMemories(userText, {
    enabled: cfg.memoryEnabled === true,
    maxItems: Math.max(0, cfg.memoryMaxItems ?? 3),
    includePrivate: cfg.memoryIncludePrivate === true,
  })

  const messages: Message[] = [{ role: 'system', content: HERMES_ANSWER_SYSTEM }]
  if (memories.length > 0) {
    messages.push({
      role: 'system',
      content: `Relevant memory:\n${memories.map((entry, i) => `${i + 1}. ${entry}`).join('\n')}`,
    })
  }
  messages.push({ role: 'user', content: userText })

  let fullText = ''
  try {
    console.log('[streamAnswerToRenderer] using provider:', provider.name)
    const stream = await provider.chat(messages, undefined, { signal })
    console.log('[streamAnswerToRenderer] stream started')
    for await (const delta of stream) {
      if (signal?.aborted) {
        console.log('[streamAnswerToRenderer] signal aborted')
        done()
        return
      }
      if (delta.text) {
        fullText += delta.text
        token(delta.text)
      }
    }
    console.log('[streamAnswerToRenderer] stream finished, full text length:', fullText.length)
    if (cfg.memoryEnabled === true) {
      rememberMemory(`User: ${userText}\nAssistant: ${fullText.slice(0, 1200)}`, 'conversation')
    }
    done()
  } catch (e) {
    if (signal?.aborted) {
      console.log('[streamAnswerToRenderer] aborted in catch')
      done()
      return
    }
    console.error('[streamAnswerToRenderer] error:', e)
    err(e instanceof Error ? e.message : String(e))
    done()
  }
}
