import type { AiModelCapability, AiProviderModel, LlmConfigRecord, ProviderId } from './llmConfig'

export const AI_PROVIDER_ROWS: Array<{ id: ProviderId; title: string; subtitle: string }> = [
  { id: 'openai', title: 'OpenAI', subtitle: 'Official Chat Completions API' },
  { id: 'deepseek', title: 'DeepSeek', subtitle: 'DeepSeek V4, V3, and R1' },
  { id: 'openai-compatible', title: 'OpenAI Compatible', subtitle: 'Custom OpenAI-style endpoint' },
  { id: 'gemini', title: 'Gemini', subtitle: 'Google Gemini via OpenAI-compatible API' },
  { id: 'anthropic', title: 'Anthropic', subtitle: 'Claude via the official API' },
  { id: 'ollama', title: 'Ollama', subtitle: 'Local models on this machine' },
  { id: 'copilot', title: 'GitHub Copilot', subtitle: 'Copilot Chat access token' },
  { id: 'opencode', title: 'OpenCode', subtitle: 'opencode.ai through the CLI' },
]

export const AI_CAPABILITIES: Array<{ id: AiModelCapability; label: string }> = [
  { id: 'vision', label: 'Vision' },
  { id: 'thinking', label: 'Thinking' },
  { id: 'tools', label: 'Tools' },
]

export const RECOMMENDED_AI_MODEL: Record<ProviderId, string> = {
  openai: 'gpt-4o-mini',
  deepseek: 'deepseek-v4-flash',
  'openai-compatible': 'gpt-4o-mini',
  gemini: 'gemini-2.0-flash',
  anthropic: 'claude-3-5-haiku-20241022',
  ollama: 'llama3.2',
  copilot: 'gpt-4o',
  opencode: 'opencode/big-pickle',
}

export const DEFAULT_BASE_URL: Partial<Record<ProviderId, string>> = {
  openai: 'https://api.openai.com/v1',
  deepseek: 'https://api.deepseek.com',
  'openai-compatible': 'https://api.openai.com/v1',
  gemini: 'https://generativelanguage.googleapis.com/v1beta/openai',
  anthropic: 'https://api.anthropic.com',
  ollama: 'http://localhost:11434',
}

export const DEFAULT_PROVIDER_MODELS: Record<ProviderId, AiProviderModel[]> = {
  openai: [
    { id: 'gpt-4o-mini', capabilities: ['vision', 'tools'], contextWindow: 128000 },
    { id: 'gpt-4o', capabilities: ['vision', 'tools'], contextWindow: 128000 },
    { id: 'o3-mini', capabilities: ['thinking', 'tools'], contextWindow: 200000 },
  ],
  deepseek: [
    { id: 'deepseek-v4-flash', capabilities: ['tools'], contextWindow: 128000 },
    { id: 'deepseek-v4-pro', capabilities: ['thinking', 'tools'], contextWindow: 128000 },
    { id: 'deepseek-reasoner', capabilities: ['thinking'], contextWindow: 64000 },
  ],
  'openai-compatible': [
    { id: 'gpt-4o-mini', capabilities: ['vision', 'tools'], contextWindow: 128000 },
  ],
  gemini: [
    { id: 'gemini-2.0-flash', capabilities: ['vision', 'tools'], contextWindow: 1000000 },
    { id: 'gemini-1.5-pro', capabilities: ['vision', 'thinking', 'tools'], contextWindow: 2000000 },
  ],
  anthropic: [
    { id: 'claude-3-5-haiku-20241022', capabilities: ['vision', 'tools'], contextWindow: 200000 },
    { id: 'claude-3-5-sonnet-20241022', capabilities: ['vision', 'thinking', 'tools'], contextWindow: 200000 },
  ],
  ollama: [
    { id: 'llama3.2', capabilities: ['tools'], contextWindow: 128000 },
    { id: 'llava', capabilities: ['vision'], contextWindow: 32000 },
  ],
  copilot: [
    { id: 'gpt-4o', capabilities: ['vision', 'tools'], contextWindow: 128000 },
    { id: 'claude-3.5-sonnet', capabilities: ['thinking', 'tools'], contextWindow: 200000 },
  ],
  opencode: [
    { id: 'opencode/big-pickle', capabilities: ['thinking', 'tools'], contextWindow: 128000 },
  ],
}

export function providerTitle(id: ProviderId): string {
  return AI_PROVIDER_ROWS.find((provider) => provider.id === id)?.title ?? id
}

export function isAiProviderConfigured(config: LlmConfigRecord, provider: ProviderId): boolean {
  const models = config.providerModels?.[provider] ?? DEFAULT_PROVIDER_MODELS[provider]
  if (models.length === 0) return false
  if (provider === 'ollama' || provider === 'opencode') return true

  const providerConfig = config.providerConfigs?.[provider]
  const isActiveLegacyProvider = config.provider === provider
  if (provider === 'gemini') {
    const apiKey =
      providerConfig?.geminiApiKey ??
      (isActiveLegacyProvider ? config.geminiApiKey ?? config.apiKey ?? '' : '')
    return Boolean(apiKey.trim())
  }
  if (provider === 'copilot') {
    const token =
      providerConfig?.copilotGithubToken ??
      (isActiveLegacyProvider ? config.copilotGithubToken ?? '' : '')
    return Boolean(token.trim())
  }

  const apiKey = providerConfig?.apiKey ?? (isActiveLegacyProvider ? config.apiKey ?? '' : '')
  return Boolean(apiKey.trim())
}

export function inferCapabilities(modelId: string): AiModelCapability[] {
  const lower = modelId.toLowerCase()
  const caps: AiModelCapability[] = []
  if (/vision|vl|llava|gpt-4o|gemini|claude/.test(lower)) caps.unshift('vision')
  if (/reason|think|r1|o\d|sonnet|pro|v4-pro|claude|deepseek/.test(lower)) caps.push('thinking')
  if (!/embed|whisper|tts/.test(lower)) caps.push('tools')
  return Array.from(new Set(caps))
}

export function normalizeModelList(models: AiProviderModel[], fallbackId: string): AiProviderModel[] {
  const seen = new Set<string>()
  const normalized: AiProviderModel[] = models
    .map((model) => {
      const next: AiProviderModel = {
        id: String(model.id || '').trim(),
        capabilities: Array.isArray(model.capabilities)
          ? model.capabilities.filter(
              (capability): capability is AiModelCapability =>
                capability === 'vision' || capability === 'thinking' || capability === 'tools'
            )
          : inferCapabilities(model.id),
      }
      if (typeof model.contextWindow === 'number' && Number.isFinite(model.contextWindow)) {
        next.contextWindow = Math.max(0, Math.round(model.contextWindow))
      }
      return next
    })
    .filter((model) => {
      if (!model.id || seen.has(model.id)) return false
      seen.add(model.id)
      return true
    })

  if (!seen.has(fallbackId)) {
    normalized.unshift({ id: fallbackId, capabilities: inferCapabilities(fallbackId) })
  }

  return normalized
}

export function normalizeProviderModelList(
  provider: ProviderId,
  models: AiProviderModel[]
): AiProviderModel[] {
  if (provider === 'openai-compatible') {
    return normalizeModelList(models, RECOMMENDED_AI_MODEL[provider])
  }

  const ownDefaults = new Set(DEFAULT_PROVIDER_MODELS[provider].map((model) => model.id))
  const otherDefaults = new Set<string>()
  for (const [otherProvider, otherModels] of Object.entries(DEFAULT_PROVIDER_MODELS) as Array<
    [ProviderId, AiProviderModel[]]
  >) {
    if (otherProvider === provider) continue
    for (const model of otherModels) {
      otherDefaults.add(model.id)
    }
  }

  return normalizeModelList(
    models.filter((model) => ownDefaults.has(model.id) || !otherDefaults.has(model.id)),
    RECOMMENDED_AI_MODEL[provider]
  )
}
