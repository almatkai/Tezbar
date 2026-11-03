import { heuristicChatMeta, isEmoji } from '../../shared/chatMeta'
import type { Message } from '../llm/provider'
import { getProviderForTask } from '../llm/registry'

const TITLE_SYSTEM_PROMPT = `You generate a short, clean title and a single emoji icon for a conversation.
Respond ONLY with a JSON object in this exact format:
{"title": "...", "icon": "..."}

Rules:
- "title": 2 to 5 words, max 35 characters. Do not use quotes or trailing punctuation. Summarize the core topic. Use the same language as the user query (e.g. Russian if the user asked in Russian, English if English).
- "icon": A single emoji character that best fits the subject (e.g. 💻, ⚽, ⚡, 📝, 🔍, 🎨, 🚀, 🎵, 📚, 💼, 🌐).`

export async function generateChatTitleAndIcon(
  prompt: string,
  answer?: string
): Promise<{ title: string; icon: string }> {
  const fallback = heuristicChatMeta(prompt, answer)
  try {
    const provider = getProviderForTask('chat')
    if (!provider || !(await provider.isAvailable())) {
      return fallback
    }

    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), 12_000)

    const userContent = `User query: ${prompt.slice(0, 300)}${
      answer ? `\nModel answer summary: ${answer.slice(0, 300)}` : ''
    }`

    const messages: Message[] = [
      { role: 'system', content: TITLE_SYSTEM_PROMPT },
      { role: 'user', content: userContent },
    ]

    const stream = await provider.chat(messages, undefined, { signal: controller.signal })
    let raw = ''
    for await (const delta of stream) {
      if (controller.signal.aborted) break
      if (delta.text) raw += delta.text
    }
    clearTimeout(timeout)

    const jsonMatch = raw.match(/\{[\s\S]*?\}/)
    if (jsonMatch) {
      const parsed = JSON.parse(jsonMatch[0]) as { title?: unknown; icon?: unknown }
      const candidateTitle =
        typeof parsed.title === 'string' && parsed.title.trim().length > 0
          ? parsed.title.trim().replace(/^["']|["']$/g, '').slice(0, 40)
          : fallback.title
      const candidateIcon =
        typeof parsed.icon === 'string' && parsed.icon.trim().length > 0 && isEmoji(parsed.icon.trim())
          ? parsed.icon.trim()
          : fallback.icon

      return {
        title: candidateTitle,
        icon: candidateIcon,
      }
    }
  } catch {
    // If the provider fails, times out, or returns malformed data, use the heuristic fallback
  }
  return fallback
}
