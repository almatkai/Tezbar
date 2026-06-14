import {
  DEFAULT_PROVIDER_MODELS,
  defaultModels,
  isCustomProvider,
  normalizeProviderModelList,
  recommendedModel,
} from '../../shared/aiProviders'
import type { AiProviderConfig, AiProviderModel, CustomAiProvider, LlmTask, ProviderId } from '../../shared/llmConfig'
import { AnthropicProvider } from './anthropic'
import { readRawConfig } from './configStore'
import { CopilotProvider } from './copilot'
import { OllamaProvider } from './ollama'
import { OpenAIProvider } from './openai'
import { OpenCodeProvider } from './opencode'
import type { LLMProvider } from './provider'

export type OpenRayLLMConfig = {
  provider: ProviderId
  customProviders?: CustomAiProvider[]
  providerConfigs?: Partial<Record<ProviderId, AiProviderConfig>>
  apiKey?: string
  baseURL?: string
  model?: string
  providerModels?: Partial<Record<ProviderId, AiProviderModel[]>>
  providerSelectedModels?: Partial<Record<ProviderId, string>>
  openaiCompatibleBaseURL?: string
  geminiApiKey?: string
  copilotGithubToken?: string
  copilotRefreshToken?: string
  copilotExpiresAt?: number
  githubOAuthClientId?: string
  taskProviderOverrides?: Partial<Record<LlmTask, ProviderId>>
  taskModelOverrides?: Partial<Record<LlmTask, string>>
  memoryEnabled?: boolean
  memoryMaxItems?: number
  memoryIncludePrivate?: boolean
  aiActionRequirePermission?: boolean
  aiActionRedactionEnabled?: boolean
  uiStateRetentionMs?: number
}

type PiProviderBridge = {
  modelPattern: string
  providerJson: string
}

const DEFAULT_OLLAMA_BASE = 'http://localhost:11434'
const DEFAULT_OLLAMA_MODEL = 'llama3.2'
const DEFAULT_GEMINI_BASE = 'https://generativelanguage.googleapis.com/v1beta/openai'
const DEFAULT_GEMINI_MODEL = 'gemini-2.0-flash'
const DEFAULT_DEEPSEEK_BASE = 'https://api.deepseek.com'
const DEFAULT_DEEPSEEK_MODEL = 'deepseek-v4-flash'

function normalizeCustomProviders(raw: unknown): CustomAiProvider[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((value) => {
    if (!value || typeof value !== 'object') return []
    const entry = value as Record<string, unknown>
    const id = typeof entry.id === 'string' ? entry.id.trim() : ''
    const title = typeof entry.title === 'string' ? entry.title.trim() : ''
    if (!id.startsWith('custom:') || !title) return []
    return [{
      id: id as `custom:${string}`,
      title,
      subtitle: typeof entry.subtitle === 'string' ? entry.subtitle.trim() : undefined,
    }]
  })
}

function providerIds(customProviders: CustomAiProvider[]): ProviderId[] {
  return [...(Object.keys(DEFAULT_PROVIDER_MODELS) as ProviderId[]), ...customProviders.map((provider) => provider.id)]
}

function normalizeProviderModels(raw: unknown, ids: ProviderId[]): Partial<Record<ProviderId, AiProviderModel[]>> | undefined {
  if (!raw || typeof raw !== 'object') return undefined
  const result: Partial<Record<ProviderId, AiProviderModel[]>> = {}
  for (const provider of ids) {
    const models = (raw as Partial<Record<ProviderId, unknown>>)[provider]
    if (!Array.isArray(models)) continue
    result[provider] = normalizeProviderModelList(provider, models as AiProviderModel[])
  }
  return result
}

function normalizeProviderSelectedModels(raw: unknown, ids: ProviderId[]): Partial<Record<ProviderId, string>> | undefined {
  if (!raw || typeof raw !== 'object') return undefined
  const result: Partial<Record<ProviderId, string>> = {}
  for (const provider of ids) {
    const value = (raw as Partial<Record<ProviderId, unknown>>)[provider]
    if (typeof value === 'string' && value.trim()) result[provider] = value.trim()
  }
  return result
}

function normalizeProviderConfigs(raw: unknown, ids: ProviderId[]): Partial<Record<ProviderId, AiProviderConfig>> | undefined {
  if (!raw || typeof raw !== 'object') return undefined
  const result: Partial<Record<ProviderId, AiProviderConfig>> = {}
  for (const provider of ids) {
    const value = (raw as Partial<Record<ProviderId, unknown>>)[provider]
    if (!value || typeof value !== 'object') continue
    const config = value as Record<string, unknown>
    result[provider] = {
      apiKey: typeof config.apiKey === 'string' ? config.apiKey : undefined,
      baseURL: typeof config.baseURL === 'string' ? config.baseURL : undefined,
      openaiCompatibleBaseURL:
        typeof config.openaiCompatibleBaseURL === 'string' ? config.openaiCompatibleBaseURL : undefined,
      geminiApiKey: typeof config.geminiApiKey === 'string' ? config.geminiApiKey : undefined,
      copilotGithubToken:
        typeof config.copilotGithubToken === 'string' ? config.copilotGithubToken : undefined,
      githubOAuthClientId:
        typeof config.githubOAuthClientId === 'string' ? config.githubOAuthClientId : undefined,
    }
  }
  return result
}

function normalizeFromRaw(raw: Record<string, unknown>): OpenRayLLMConfig {
  const customProviders = normalizeCustomProviders(raw.customProviders)
  const ids = providerIds(customProviders)
  const p = raw.provider
  const hasCopilotToken = typeof raw.copilotGithubToken === 'string' && raw.copilotGithubToken.length > 0
  const provider: ProviderId =
    (typeof p === 'string' && customProviders.some((provider) => provider.id === p)) ||
      p === 'openai' ||
      p === 'openai-compatible' ||
      p === 'anthropic' ||
      p === 'ollama' ||
      p === 'copilot' ||
      p === 'gemini' ||
      p === 'opencode' ||
      p === 'deepseek'
      ? (p as ProviderId)
      : hasCopilotToken
        ? 'copilot'
        : 'ollama'
  const providerModels = normalizeProviderModels(raw.providerModels, ids)
  const providerSelectedModels = normalizeProviderSelectedModels(raw.providerSelectedModels, ids)
  const providerConfigs = normalizeProviderConfigs(raw.providerConfigs, ids)
  const selectedModel = providerSelectedModels?.[provider]
  const providerConfig = providerConfigs?.[provider] ?? {}
  const allowLegacyProviderFields = !providerConfigs || Object.keys(providerConfigs).length === 0

  return {
    provider,
    customProviders,
    providerConfigs,
    apiKey:
      providerConfig.apiKey ??
      (allowLegacyProviderFields && typeof raw.apiKey === 'string' ? raw.apiKey : undefined),
    baseURL:
      providerConfig.baseURL ??
      (allowLegacyProviderFields && typeof raw.baseURL === 'string' ? raw.baseURL : undefined),
    openaiCompatibleBaseURL:
      providerConfig.openaiCompatibleBaseURL ??
      (allowLegacyProviderFields && typeof raw.openaiCompatibleBaseURL === 'string'
        ? raw.openaiCompatibleBaseURL
        : undefined),
    geminiApiKey:
      providerConfig.geminiApiKey ??
      (allowLegacyProviderFields && typeof raw.geminiApiKey === 'string' ? raw.geminiApiKey : undefined),
    model: selectedModel ?? (typeof raw.model === 'string' ? raw.model : undefined),
    providerModels,
    providerSelectedModels,
    copilotGithubToken:
      providerConfig.copilotGithubToken ??
      (allowLegacyProviderFields && typeof raw.copilotGithubToken === 'string'
        ? raw.copilotGithubToken
        : undefined),
    copilotRefreshToken: typeof raw.copilotRefreshToken === 'string' ? raw.copilotRefreshToken : undefined,
    copilotExpiresAt: typeof raw.copilotExpiresAt === 'number' ? raw.copilotExpiresAt : undefined,
    githubOAuthClientId:
      providerConfig.githubOAuthClientId ??
      (allowLegacyProviderFields && typeof raw.githubOAuthClientId === 'string'
        ? raw.githubOAuthClientId
        : undefined),
    taskProviderOverrides:
      typeof raw.taskProviderOverrides === 'object' && raw.taskProviderOverrides
        ? (raw.taskProviderOverrides as Partial<Record<LlmTask, ProviderId>>)
        : undefined,
    taskModelOverrides:
      typeof raw.taskModelOverrides === 'object' && raw.taskModelOverrides
        ? (raw.taskModelOverrides as Partial<Record<LlmTask, string>>)
        : undefined,
    memoryEnabled: typeof raw.memoryEnabled === 'boolean' ? raw.memoryEnabled : undefined,
    memoryMaxItems: typeof raw.memoryMaxItems === 'number' ? raw.memoryMaxItems : undefined,
    memoryIncludePrivate: typeof raw.memoryIncludePrivate === 'boolean' ? raw.memoryIncludePrivate : undefined,
    aiActionRequirePermission:
      typeof raw.aiActionRequirePermission === 'boolean' ? raw.aiActionRequirePermission : undefined,
    aiActionRedactionEnabled:
      typeof raw.aiActionRedactionEnabled === 'boolean' ? raw.aiActionRedactionEnabled : undefined,
    uiStateRetentionMs: typeof raw.uiStateRetentionMs === 'number' ? raw.uiStateRetentionMs : undefined,
  }
}

export function readLLMConfig(): OpenRayLLMConfig {
  const raw = readRawConfig()
  if (Object.keys(raw).length === 0) {
    return { provider: 'ollama', baseURL: DEFAULT_OLLAMA_BASE, model: DEFAULT_OLLAMA_MODEL }
  }
  const n = normalizeFromRaw(raw)
  if (n.provider === 'ollama') {
    return {
      ...n,
      baseURL: n.baseURL ?? DEFAULT_OLLAMA_BASE,
      model: n.model ?? DEFAULT_OLLAMA_MODEL,
    }
  }
  if (n.provider === 'gemini') {
    return {
      ...n,
      baseURL: n.baseURL ?? DEFAULT_GEMINI_BASE,
      model: n.model ?? DEFAULT_GEMINI_MODEL,
    }
  }
  if (n.provider === 'deepseek') {
    return {
      ...n,
      baseURL: n.baseURL ?? DEFAULT_DEEPSEEK_BASE,
      model: n.model ?? DEFAULT_DEEPSEEK_MODEL,
    }
  }
  return n
}

export function configForProvider(cfg: OpenRayLLMConfig, provider: ProviderId): OpenRayLLMConfig {
  const providerConfig = cfg.providerConfigs?.[provider] ?? {}
  const useCurrentProviderFields = cfg.provider === provider
  const model =
    cfg.providerSelectedModels?.[provider] ?? (useCurrentProviderFields ? cfg.model : undefined)
  const next: OpenRayLLMConfig = {
    ...cfg,
    provider,
    model,
    apiKey: providerConfig.apiKey ?? (useCurrentProviderFields ? cfg.apiKey : undefined),
    baseURL: providerConfig.baseURL ?? (useCurrentProviderFields ? cfg.baseURL : undefined),
    openaiCompatibleBaseURL:
      providerConfig.openaiCompatibleBaseURL ??
      (useCurrentProviderFields ? cfg.openaiCompatibleBaseURL : undefined),
    geminiApiKey:
      providerConfig.geminiApiKey ?? (useCurrentProviderFields ? cfg.geminiApiKey : undefined),
    copilotGithubToken:
      providerConfig.copilotGithubToken ??
      (useCurrentProviderFields ? cfg.copilotGithubToken : undefined),
    githubOAuthClientId:
      providerConfig.githubOAuthClientId ??
      (useCurrentProviderFields ? cfg.githubOAuthClientId : undefined),
  }

  if (provider === 'ollama') {
    return { ...next, baseURL: next.baseURL ?? DEFAULT_OLLAMA_BASE, model: next.model ?? DEFAULT_OLLAMA_MODEL }
  }
  if (provider === 'gemini') {
    return { ...next, baseURL: next.baseURL ?? DEFAULT_GEMINI_BASE, model: next.model ?? DEFAULT_GEMINI_MODEL }
  }
  if (provider === 'deepseek') {
    return { ...next, baseURL: next.baseURL ?? DEFAULT_DEEPSEEK_BASE, model: next.model ?? DEFAULT_DEEPSEEK_MODEL }
  }
  return {
    ...next,
    model: next.model ?? (recommendedModel(provider) || defaultModels(provider)[0]?.id),
  }
}

export function buildProviderForId(id: ProviderId, cfg: OpenRayLLMConfig): LLMProvider {
  return buildProvider(configForProvider(cfg, id))
}

function buildProvider(cfg: OpenRayLLMConfig): LLMProvider {
  if (isCustomProvider(cfg.provider)) {
    return new OpenAIProvider(
      cfg.openaiCompatibleBaseURL ?? cfg.baseURL ?? '',
      cfg.apiKey ?? '',
      cfg.model ?? '',
      cfg.customProviders?.find((provider) => provider.id === cfg.provider)?.title ?? 'Custom provider',
    )
  }
  switch (cfg.provider) {
    case 'openai':
      return new OpenAIProvider(
        cfg.baseURL ?? 'https://api.openai.com/v1',
        cfg.apiKey ?? '',
        cfg.model ?? 'gpt-4o-mini',
        'OpenAI',
      )
    case 'openai-compatible':
      return new OpenAIProvider(
        cfg.openaiCompatibleBaseURL ?? cfg.baseURL ?? 'https://api.openai.com/v1',
        cfg.apiKey ?? '',
        cfg.model ?? 'gpt-4o-mini',
        'OpenAI-compatible provider',
      )
    case 'anthropic':
      return new AnthropicProvider(
        cfg.apiKey ?? '',
        cfg.model ?? 'claude-3-5-haiku-20241022',
        cfg.baseURL,
      )
    case 'ollama':
      return new OllamaProvider(cfg.baseURL ?? DEFAULT_OLLAMA_BASE, cfg.model ?? DEFAULT_OLLAMA_MODEL)
    case 'copilot':
      return new CopilotProvider(cfg.model ?? 'gpt-4o')
    case 'gemini':
      return new OpenAIProvider(
        cfg.baseURL ?? DEFAULT_GEMINI_BASE,
        cfg.geminiApiKey ?? cfg.apiKey ?? '',
        cfg.model ?? DEFAULT_GEMINI_MODEL,
        'Gemini',
      )
    case 'opencode':
      return new OpenCodeProvider(cfg.model ?? 'opencode/big-pickle')
    case 'deepseek':
      return new OpenAIProvider(
        cfg.baseURL ?? DEFAULT_DEEPSEEK_BASE,
        cfg.apiKey ?? '',
        cfg.model ?? DEFAULT_DEEPSEEK_MODEL,
        'DeepSeek',
      )
    default:
      return new OllamaProvider(DEFAULT_OLLAMA_BASE, DEFAULT_OLLAMA_MODEL)
  }
}

let cacheKey = ''
let active: LLMProvider | null = null

export function invalidateProviderCache(): void {
  cacheKey = ''
  active = null
}

export function getProvider(): LLMProvider {
  const cfg = readLLMConfig()
  const key = JSON.stringify(cfg)
  if (active && key === cacheKey) return active
  active = buildProvider(cfg)
  cacheKey = key
  return active
}

export function getProviderForTask(task: LlmTask): LLMProvider {
  const cfg = readLLMConfig()
  const providerOverride = cfg.taskProviderOverrides?.[task]
  const modelOverride = cfg.taskModelOverrides?.[task]
  const targetProvider = providerOverride ?? cfg.provider
  const targetConfig = configForProvider(cfg, targetProvider)
  const merged: OpenRayLLMConfig = {
    ...targetConfig,
    model: modelOverride ?? targetConfig.model,
  }
  return buildProvider(merged)
}

export function getSelectedPiModelPattern(): string | undefined {
  const cfg = readLLMConfig()
  const model = cfg.model?.trim()
  if (!model) return undefined

  const provider = cfg.provider
  if (provider === 'opencode') {
    if (model.startsWith('opencode/opencode/')) return model
    if (model.startsWith('opencode/')) return `opencode/${model}`
    return `opencode/opencode/${model}`
  }
  if (model.startsWith(`${provider}/`)) return model
  if (model.includes('/')) return model
  return `${provider}/${model}`
}

function stripProviderPrefix(model: string, provider: ProviderId): string {
  const prefix = `${provider}/`
  let normalized = model.trim()
  while (normalized.startsWith(prefix)) {
    normalized = normalized.slice(prefix.length)
  }
  return normalized
}

function openAiCompatBaseUrl(cfg: OpenRayLLMConfig): string | undefined {
  if (cfg.provider === 'openai') return cfg.baseURL ?? 'https://api.openai.com/v1'
  if (cfg.provider === 'openai-compatible') return cfg.openaiCompatibleBaseURL ?? cfg.baseURL
  if (cfg.provider === 'gemini') return cfg.baseURL ?? DEFAULT_GEMINI_BASE
  if (cfg.provider === 'deepseek') return cfg.baseURL ?? DEFAULT_DEEPSEEK_BASE
  if (isCustomProvider(cfg.provider)) return cfg.openaiCompatibleBaseURL ?? cfg.baseURL
  if (cfg.provider === 'ollama') {
    const base = cfg.baseURL ?? DEFAULT_OLLAMA_BASE
    return base.endsWith('/v1') ? base : `${base.replace(/\/+$/, '')}/v1`
  }
  return undefined
}

function piApiKey(cfg: OpenRayLLMConfig): string | undefined {
  if (cfg.provider === 'gemini') return cfg.geminiApiKey ?? cfg.apiKey
  if (cfg.provider === 'ollama') return 'ollama'
  return cfg.apiKey
}

export function getSelectedPiProviderBridge(): PiProviderBridge | undefined {
  const cfg = readLLMConfig()
  const model = cfg.model?.trim()
  if (!model) return undefined

  const modelId = stripProviderPrefix(model, cfg.provider)
  if (!modelId) return undefined

  const isAnthropic = cfg.provider === 'anthropic'
  const baseUrl = isAnthropic ? cfg.baseURL ?? 'https://api.anthropic.com' : openAiCompatBaseUrl(cfg)
  const apiKey = isAnthropic ? cfg.apiKey : piApiKey(cfg)
  if (!baseUrl || !apiKey) return undefined

  const providerJson = JSON.stringify({
    baseUrl,
    apiKey,
    api: isAnthropic ? 'anthropic-messages' : 'openai-completions',
    authHeader: true,
    models: [
      {
        id: modelId,
        name: `Tezbar ${cfg.provider} ${modelId}`,
        reasoning: /reason|think|r1|o\d|gpt-5|claude|deepseek/i.test(modelId),
        input: ['text'],
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
        contextWindow: 128000,
        maxTokens: 8192,
      },
    ],
  })

  return {
    modelPattern: `tezbar/${modelId}`,
    providerJson,
  }
}
