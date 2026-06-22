import { isCustomProvider } from '../../shared/aiProviders'
import type { ProviderId } from '../../shared/llmConfig'
import { CopilotProvider } from './copilot'
import { configForProvider, readLLMConfig } from './registry'

function trimSlash(url: string): string {
  return url.replace(/\/+$/, '')
}

function modelsUrl(baseURL: string): string {
  const base = trimSlash(baseURL)
  if (base.endsWith('/chat/completions')) {
    return `${base.slice(0, -'/chat/completions'.length)}/models`
  }
  return `${base}/models`
}

function uniqSorted(ids: string[]): string[] {
  return Array.from(new Set(ids.filter(Boolean))).sort((a, b) => a.localeCompare(b))
}

function extractModelIds(json: unknown): string[] {
  if (!json || typeof json !== 'object') return []
  const o = json as Record<string, unknown>

  if (Array.isArray(o.data)) {
    const ids: string[] = []
    for (const item of o.data) {
      if (item && typeof item === 'object' && 'id' in item) {
        const id = (item as { id: unknown }).id
        if (typeof id === 'string' && id) ids.push(id)
      }
    }
    if (ids.length) return uniqSorted(ids)
  }

  if (Array.isArray(o.models)) {
    const ids: string[] = []
    for (const item of o.models) {
      if (!item || typeof item !== 'object') continue
      const m = item as Record<string, unknown>
      const name = typeof m.name === 'string' ? m.name : typeof m.model === 'string' ? m.model : ''
      if (name) ids.push(name)
    }
    if (ids.length) return uniqSorted(ids)
  }

  return []
}

const COPILOT_MODELS = 'https://api.githubcopilot.com/models'

async function fetchCopilotModelIds(accessToken: string, signal?: AbortSignal): Promise<string[]> {
  if (!accessToken.trim()) return []
  try {
    const res = await fetch(COPILOT_MODELS, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${accessToken}`,
        'Editor-Version': 'Tezbar/0.1.0',
        'Copilot-Integration-Id': 'vscode-chat',
        Accept: 'application/json',
      },
      signal: signal ?? AbortSignal.timeout(12_000),
    })
    if (!res.ok) return []
    const json: unknown = await res.json()
    return extractModelIds(json)
  } catch {
    return []
  }
}

export async function listModelsForProvider(id: ProviderId, signal?: AbortSignal): Promise<string[]> {
  const cfg = configForProvider(readLLMConfig(), id)

  if (isCustomProvider(id)) {
    const base = cfg.openaiCompatibleBaseURL ?? cfg.baseURL ?? ''
    const key = cfg.apiKey ?? ''
    if (!base.trim() || !key.trim()) return []
    try {
      const res = await fetch(modelsUrl(base), {
        method: 'GET',
        headers: { Authorization: `Bearer ${key}` },
        signal: signal ?? AbortSignal.timeout(12_000),
      })
      if (!res.ok) return []
      return extractModelIds(await res.json())
    } catch {
      return []
    }
  }

  switch (id) {
    case 'openai': {
      const base = cfg.baseURL ?? 'https://api.openai.com/v1'
      const key = cfg.apiKey ?? ''
      if (!key.trim()) return []
      try {
        const res = await fetch(modelsUrl(base), {
          method: 'GET',
          headers: { Authorization: `Bearer ${key}` },
          signal: signal ?? AbortSignal.timeout(12_000),
        })
        if (!res.ok) return []
        return extractModelIds(await res.json())
      } catch {
        return []
      }
    }
    case 'openai-compatible': {
      const base = cfg.openaiCompatibleBaseURL ?? cfg.baseURL ?? 'https://api.openai.com/v1'
      const key = cfg.apiKey ?? ''
      if (!key.trim()) return []
      try {
        const res = await fetch(modelsUrl(base), {
          method: 'GET',
          headers: { Authorization: `Bearer ${key}` },
          signal: signal ?? AbortSignal.timeout(12_000),
        })
        if (!res.ok) return []
        return extractModelIds(await res.json())
      } catch {
        return []
      }
    }
    case 'anthropic': {
      const apiBase = trimSlash(cfg.baseURL ?? 'https://api.anthropic.com')
      const key = cfg.apiKey ?? ''
      if (!key.trim()) return []
      try {
        const res = await fetch(`${apiBase}/v1/models`, {
          method: 'GET',
          headers: {
            'x-api-key': key,
            'anthropic-version': '2023-06-01',
          },
          signal: signal ?? AbortSignal.timeout(12_000),
        })
        if (!res.ok) return []
        return extractModelIds(await res.json())
      } catch {
        return []
      }
    }
    case 'ollama': {
      const base = cfg.baseURL ?? 'http://localhost:11434'
      try {
        const res = await fetch(`${trimSlash(base)}/api/tags`, {
          method: 'GET',
          signal: signal ?? AbortSignal.timeout(12_000),
        })
        if (!res.ok) return []
        return extractModelIds(await res.json())
      } catch {
        return []
      }
    }
    case 'gemini': {
      const base = cfg.baseURL ?? 'https://generativelanguage.googleapis.com/v1beta/openai'
      const key = cfg.geminiApiKey ?? cfg.apiKey ?? ''
      if (!key.trim()) return []
      try {
        const res = await fetch(modelsUrl(base), {
          method: 'GET',
          headers: { Authorization: `Bearer ${key}` },
          signal: signal ?? AbortSignal.timeout(12_000),
        })
        if (!res.ok) {
          return ['gemini-2.0-flash', 'gemini-2.5-flash', 'gemini-2.5-pro']
        }
        const ids = extractModelIds(await res.json())
        return ids.length > 0 ? ids : ['gemini-2.0-flash', 'gemini-2.5-flash', 'gemini-2.5-pro']
      } catch {
        return ['gemini-2.0-flash', 'gemini-2.5-flash', 'gemini-2.5-pro']
      }
    }
    case 'copilot': {
      const cp = new CopilotProvider(cfg.model ?? 'gpt-4o')
      const token = await cp.getAccessToken({ signal })
      return fetchCopilotModelIds(token, signal)
    }
    case 'opencode': {
      try {
        const { execFile } = await import('node:child_process')
        const { promisify } = await import('node:util')
        const execFileAsync = promisify(execFile)
        const { stdout } = await execFileAsync('opencode', ['models'], { timeout: 12_000, signal })
        const models = stdout
          .replace(/\x1b\[[0-9;]*m/g, '')
          .split('\n')
          .map((line) => line.trim())
          .filter((line) => line.startsWith('opencode/') || line.startsWith('opencode-go/'))
        return models.length > 0 ? models : ['opencode/big-pickle']
      } catch {
        return ['opencode/big-pickle']
      }
    }
    case 'deepseek': {
      const base = cfg.baseURL ?? 'https://api.deepseek.com'
      const key = cfg.apiKey ?? ''
      if (!key.trim()) return ['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-reasoner']
      try {
        const res = await fetch(modelsUrl(base), {
          method: 'GET',
          headers: { Authorization: `Bearer ${key}` },
          signal: signal ?? AbortSignal.timeout(12_000),
        })
        if (!res.ok) return ['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-reasoner']
        const ids = extractModelIds(await res.json())
        return ids.length > 0 ? ids : ['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-reasoner']
      } catch {
        return ['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-reasoner']
      }
    }
    default:
      return []
  }
}
