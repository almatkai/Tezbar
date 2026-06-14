import { readRawConfig, writeConfigPatch } from './configStore'
import { copilotApiPing, refreshGithubAccessToken } from './githubCopilotAuth'
import { parseOpenAISSE } from './openaiSse'
import type { ChatOptions, Delta, LLMProvider, Message, Tool } from './provider'

const COPILOT_CHAT = 'https://api.githubcopilot.com/chat/completions'

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

export class CopilotProvider implements LLMProvider {
  readonly name = 'copilot'

  constructor(private readonly model: string) {}

  private readTokens(): {
    access: string
    refresh?: string
    expiresAt: number
    clientId: string
  } {
    const c = readRawConfig()
    return {
      access:
        (typeof c.copilotGithubToken === 'string' ? c.copilotGithubToken : '') ||
        (typeof c.apiKey === 'string' ? c.apiKey : ''),
      refresh: typeof c.copilotRefreshToken === 'string' ? c.copilotRefreshToken : undefined,
      expiresAt: typeof c.copilotExpiresAt === 'number' ? c.copilotExpiresAt : 0,
      clientId: typeof c.githubOAuthClientId === 'string' ? c.githubOAuthClientId : '',
    }
  }

  private async refreshIfNeeded(signal?: AbortSignal): Promise<string> {
    const { access, refresh, expiresAt, clientId } = this.readTokens()
    if (!refresh || !clientId) {
      return access
    }
    const stale = expiresAt > 0 && Date.now() > expiresAt - 120_000
    if (!stale) {
      return access
    }
    const next = await refreshGithubAccessToken(refresh, clientId, signal)
    const expires_in = next.expires_in
    const patch: Record<string, unknown> = {
      copilotGithubToken: next.access_token,
      copilotRefreshToken: next.refresh_token ?? refresh,
    }
    if (expires_in !== undefined) {
      patch.copilotExpiresAt = Date.now() + expires_in * 1000
    }
    writeConfigPatch(patch)
    return next.access_token
  }

  async chat(messages: Message[], tools?: Tool[], options?: ChatOptions): Promise<AsyncIterable<Delta>> {
    console.log('[CopilotProvider] chat request to', COPILOT_CHAT)
    const token = await this.refreshIfNeeded(options?.signal)
    if (!token.trim()) {
      console.error('[CopilotProvider] missing token')
      throw new Error('GitHub Copilot: missing token. Add a PAT or complete device sign-in in Providers.')
    }
    const body: Record<string, unknown> = {
      model: this.model,
      messages: toOpenAIMessages(messages),
      stream: true,
    }
    if (tools?.length) {
      body.tools = toOpenAITools(tools)
    }
    console.log('[CopilotProvider] payload size:', JSON.stringify(body).length, 'bytes')
    const res = await fetch(COPILOT_CHAT, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
        'Editor-Version': 'TezBar/0.1.0',
        'Copilot-Integration-Id': 'vscode-chat',
      },
      body: JSON.stringify(body),
      signal: options?.signal,
    })
    console.log('[CopilotProvider] response status:', res.status, res.statusText)
    if (!res.ok) {
      const errBody = await res.text().catch(() => '')
      console.error('[CopilotProvider] error body:', errBody)
      throw new Error(`Copilot error ${res.status}: ${errBody.slice(0, 500)}`)
    }
    return parseOpenAISSE(res, options?.signal)
  }

  async isAvailable(): Promise<boolean> {
    const { access } = this.readTokens()
    if (!access.trim()) return false
    const ping = await copilotApiPing(access)
    if (ping) return true
    return access.length > 20
  }

  /** Bearer token after optional OAuth refresh (reads tokens from config on disk). */
  async getAccessToken(options?: ChatOptions): Promise<string> {
    return this.refreshIfNeeded(options?.signal)
  }
}
