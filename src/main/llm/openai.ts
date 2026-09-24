import type { ChatOptions, Delta, LLMProvider, Message, Tool } from './provider'
import { formatLlmErrorMessage } from '../../shared/llmErrors'
import { parseOpenAISSE } from './openaiSse'

function normalizeBaseUrl(url: string): string {
  let cleaned = url.trim().replace(/\/+$/, '')
  cleaned = cleaned.replace(/^(https?:\/\/)0\.0\.0\.0(?::(\d+))?/, (_match, proto, port) => {
    return `${proto}127.0.0.1${port ? `:${port}` : ''}`
  })
  return cleaned
}

function trimSlash(url: string): string {
  return normalizeBaseUrl(url)
}

function chatCompletionsUrl(baseURL: string): string {
  const base = normalizeBaseUrl(baseURL)
  return base.endsWith('/chat/completions') ? base : `${base}/chat/completions`
}

function modelsUrl(baseURL: string): string {
  const base = normalizeBaseUrl(baseURL)
  if (base.endsWith('/chat/completions')) {
    return `${base.slice(0, -'/chat/completions'.length)}/models`
  }
  return `${base}/models`
}

function toOpenAIMessages(messages: Message[]): Array<Record<string, unknown>> {
  return messages.map((m) => ({ role: m.role, content: m.content }))
}

function toOpenAITools(tools: Tool[]): Array<Record<string, unknown>> {
  return tools.map((t) => ({
    type: 'function',
    function: {
      name: t.name,
      description: t.description,
      parameters: t.parameters,
    },
  }))
}

export class OpenAIProvider implements LLMProvider {
  readonly name = 'openai'

  constructor(
    private readonly baseURL: string,
    private readonly apiKey: string,
    private readonly model: string,
    private readonly providerLabel = 'OpenAI',
  ) {}

  async chat(messages: Message[], tools?: Tool[], options?: ChatOptions): Promise<AsyncIterable<Delta>> {
    const cleanBase = normalizeBaseUrl(this.baseURL)
    if (!cleanBase) {
      throw new Error(
        formatLlmErrorMessage(
          `Base URL is not configured for ${this.providerLabel}. Open Settings > AI and enter the endpoint Base URL (e.g. http://127.0.0.1:8080/v1).`,
          this.providerLabel
        )
      )
    }
    const url = chatCompletionsUrl(cleanBase)
    const body: Record<string, unknown> = {
      model: this.model,
      messages: toOpenAIMessages(messages),
      stream: true,
      max_tokens: 8192,
    }
    if (tools?.length) {
      body.tools = toOpenAITools(tools)
    }
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    }
    if (this.apiKey.trim()) {
      headers.Authorization = `Bearer ${this.apiKey.trim()}`
    }
    const res = await fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
      signal: options?.signal,
    })
    if (!res.ok) {
      const errBody = await res.text().catch(() => '')
      throw new Error(
        formatLlmErrorMessage(
          `${this.providerLabel} error ${res.status}: ${errBody.slice(0, 500)}`,
          this.providerLabel
        )
      )
    }
    return parseOpenAISSE(res, options?.signal)
  }

  async isAvailable(): Promise<boolean> {
    const normBase = normalizeBaseUrl(this.baseURL)
    if (!normBase) return false
    try {
      const url = modelsUrl(normBase)
      const headers: Record<string, string> = {}
      if (this.apiKey.trim()) {
        headers.Authorization = `Bearer ${this.apiKey.trim()}`
      }
      const res = await fetch(url, {
        method: 'GET',
        headers,
        signal: AbortSignal.timeout(4000),
      })
      if (res.ok) return true
      return res.status < 500
    } catch {
      return false
    }
  }
}
