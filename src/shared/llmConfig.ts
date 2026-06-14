export type BuiltInProviderId = 'openai' | 'openai-compatible' | 'anthropic' | 'ollama' | 'copilot' | 'gemini' | 'opencode' | 'deepseek'

export type ProviderId = BuiltInProviderId | `custom:${string}`

export type LlmTask = 'chat' | 'search' | 'action' | 'voice'

export type AiModelCapability = 'vision' | 'thinking' | 'tools'

export type AiProviderModel = {
  id: string
  capabilities: AiModelCapability[]
  contextWindow?: number
}

export type AiProviderConfig = {
  apiKey?: string
  baseURL?: string
  openaiCompatibleBaseURL?: string
  geminiApiKey?: string
  copilotGithubToken?: string
  githubOAuthClientId?: string
}

export type CustomAiProvider = {
  id: `custom:${string}`
  title: string
  subtitle?: string
}

export type LlmConfigRecord = {
  provider?: ProviderId
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
  voiceSttModelId?: 'moonshine-base-en' | 'whisper-base' | 'whisper-small'
  raymesHotkey?: string
  /** Milliseconds to remember palette UI (e.g. Providers) after hide. Default 60000. Use 0 to always reset. */
  uiStateRetentionMs?: number
  settingsInitialTab?: 'general' | 'ai' | 'voice' | 'permissions' | 'storage' | 'advanced'
}
