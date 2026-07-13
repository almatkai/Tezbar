import type { AiActionRequest } from '../../shared/ipc'
import type { Message } from './provider'
import { getProviderForTask } from './registry'
import { redactSensitiveText } from '../safety/redaction'

export async function runAiActionMode(
  req: AiActionRequest,
  options?: { signal?: AbortSignal },
): Promise<{ ok: boolean; output: string }> {
  const provider = getProviderForTask('action')
  const selectedText = req.selectedText ? (req.redactSensitive === false ? req.selectedText : redactSensitiveText(req.selectedText)) : ''
  const appContext = req.appContext ? (req.redactSensitive === false ? req.appContext : redactSensitiveText(req.appContext)) : ''

  const messages: Message[] = [
    {
      role: 'system',
      content:
        'You are Tezbar Action Mode. Produce concise, executable steps and concrete output. Never execute system actions unless explicitly allowed.',
    },
    {
      role: 'user',
      content: [
        `Instruction: ${req.instruction}`,
        req.allowAutomation ? 'Automation permission: granted' : 'Automation permission: denied',
        selectedText ? `Selected text:\n${selectedText}` : 'Selected text: (none)',
        appContext ? `App context:\n${appContext}` : 'App context: (none)',
      ].join('\n\n'),
    },
  ]

  const stream = await provider.chat(messages, undefined, { signal: options?.signal })
  let output = ''
  for await (const delta of stream) {
    if (delta.text) output += delta.text
  }

  return {
    ok: true,
    output: output.trim(),
  }
}
