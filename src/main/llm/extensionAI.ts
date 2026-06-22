import type { Message } from './provider'

const EXTENSION_AI_TIMEOUT_MS = 60_000
const EXTENSION_AI_SYSTEM =
  'You are answering an AI request from a Raycast-compatible extension. Return only the requested result, without commentary or tool use.'

export async function askExtensionAI(prompt: string): Promise<string> {
  const normalizedPrompt = String(prompt || '').trim()
  if (!normalizedPrompt) throw new Error('AI prompt is required')

  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), EXTENSION_AI_TIMEOUT_MS)
  const messages: Message[] = [
    { role: 'system', content: EXTENSION_AI_SYSTEM },
    { role: 'user', content: normalizedPrompt },
  ]

  try {
    const { getProviderForTask } = await import('./registry')
    const provider = getProviderForTask('action')
    if (!(await provider.isAvailable())) {
      throw new Error('The configured AI provider is unavailable')
    }

    const stream = await provider.chat(messages, undefined, { signal: controller.signal })
    let answer = ''
    for await (const delta of stream) {
      if (delta.text) answer += delta.text
    }
    return answer.trim()
  } catch (error) {
    if (controller.signal.aborted) {
      throw new Error('Extension AI request timed out')
    }
    throw error
  } finally {
    clearTimeout(timeout)
  }
}
