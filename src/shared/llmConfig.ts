import type { VoiceModelId } from './voice'

export const DEFAULT_EXTENSION_RUNTIME_TIMEOUT_MS = 5 * 60 * 1000

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
  voiceSttModelId?: VoiceModelId
  raymesHotkey?: string
  /** Milliseconds to remember palette UI (e.g. Providers) after hide. Default 60000. Use 0 to always reset. */
  uiStateRetentionMs?: number
  /** Milliseconds of inactivity before an extension view returns to the main CommandBar. 0 disables it. */
  extensionRuntimeTimeoutMs?: number
  /** Milliseconds of inactivity before AI mode returns to the main CommandBar. 0 disables it. */
  aiModeTimeoutMs?: number
  /** Milliseconds of inactivity before terminal mode returns to the main CommandBar. 0 disables it. */
  terminalModeTimeoutMs?: number
  settingsInitialTab?: 'general' | 'ai' | 'voice' | 'knowledge' | 'extensions' | 'permissions' | 'storage' | 'advanced'
}
