import { describe, expect, it, vi } from 'vitest'
import { readRawConfig } from './configStore'
import {
  configForProvider,
  getSelectedPiModelPattern,
  getSelectedPiProviderBridge,
  readLLMConfig,
  type OpenRayLLMConfig,
} from './registry'

vi.mock('./configStore', () => ({
  readRawConfig: vi.fn(),
}))

describe('configForProvider', () => {
  it('does not inherit an active DeepSeek endpoint or credentials for Gemini', () => {
    const deepseek: OpenRayLLMConfig = {
      provider: 'deepseek',
      baseURL: 'https://api.deepseek.com',
      apiKey: 'deepseek-key',
      model: 'deepseek-v4-flash',
      providerConfigs: {
        deepseek: { baseURL: 'https://api.deepseek.com', apiKey: 'deepseek-key' },
      },
      providerSelectedModels: {
        deepseek: 'deepseek-v4-flash',
      },
    }

    expect(configForProvider(deepseek, 'gemini')).toMatchObject({
      provider: 'gemini',
      baseURL: 'https://generativelanguage.googleapis.com/v1beta/openai',
      model: 'gemini-2.0-flash',
      apiKey: undefined,
      geminiApiKey: undefined,
    })
  })

  it('uses only the selected providers own saved endpoint and model', () => {
    const config: OpenRayLLMConfig = {
      provider: 'deepseek',
      baseURL: 'https://api.deepseek.com',
      apiKey: 'deepseek-key',
      model: 'deepseek-v4-flash',
      providerConfigs: {
        deepseek: { baseURL: 'https://api.deepseek.com', apiKey: 'deepseek-key' },
        gemini: { baseURL: 'https://gemini.example/v1', geminiApiKey: 'gemini-key' },
      },
      providerSelectedModels: {
        deepseek: 'deepseek-v4-flash',
        gemini: 'gemini-custom',
      },
    }

    expect(configForProvider(config, 'gemini')).toMatchObject({
      provider: 'gemini',
      baseURL: 'https://gemini.example/v1',
      model: 'gemini-custom',
      apiKey: undefined,
      geminiApiKey: 'gemini-key',
    })
  })

  it('does not rehydrate Gemini from legacy DeepSeek flat fields', () => {
    vi.mocked(readRawConfig).mockReturnValue({
      provider: 'gemini',
      apiKey: 'deepseek-key',
      baseURL: 'https://api.deepseek.com',
      model: 'gemini-2.0-flash',
      providerConfigs: {
        deepseek: { baseURL: 'https://api.deepseek.com', apiKey: 'deepseek-key' },
      },
      providerSelectedModels: {
        gemini: 'gemini-2.0-flash',
      },
    })

    expect(readLLMConfig()).toMatchObject({
      provider: 'gemini',
      baseURL: 'https://generativelanguage.googleapis.com/v1beta/openai',
      model: 'gemini-2.0-flash',
      apiKey: undefined,
      geminiApiKey: undefined,
    })
  })

  it('uses chat task overrides when building Pi model settings', () => {
    vi.mocked(readRawConfig).mockReturnValue({
      provider: 'copilot',
      model: 'gpt-4o',
      providerConfigs: {
        copilot: { copilotGithubToken: 'copilot-token' },
        deepseek: { baseURL: 'https://api.deepseek.com', apiKey: 'deepseek-key' },
      },
      providerSelectedModels: {
        copilot: 'gpt-4o',
        deepseek: 'deepseek-v4-flash',
      },
      taskProviderOverrides: { chat: 'deepseek' },
      taskModelOverrides: { chat: 'deepseek-v4-flash' },
    })

    expect(getSelectedPiModelPattern('chat')).toBe('deepseek/deepseek-v4-flash')
    expect(getSelectedPiProviderBridge('chat')?.providerJson).toContain('https://api.deepseek.com')
  })

  it('routes TokenRouter models through the OpenAI-compatible Pi provider', () => {
    vi.mocked(readRawConfig).mockReturnValue({
      provider: 'tokenrouter',
      providerConfigs: {
        tokenrouter: {
          baseURL: 'https://api.tokenrouter.com/v1',
          apiKey: 'tokenrouter-key',
        },
      },
      providerSelectedModels: {
        tokenrouter: 'moonshotai/kimi-k3-free',
      },
      providerModels: {
        tokenrouter: [{ id: 'moonshotai/kimi-k3-free', capabilities: ['tools'] }],
      },
    })

    expect(getSelectedPiModelPattern('chat')).toBe('tokenrouter/moonshotai/kimi-k3-free')

    const bridge = getSelectedPiProviderBridge('chat')
    const provider = JSON.parse(bridge?.providerJson ?? '{}') as {
      api?: string
      authHeader?: boolean
      baseUrl?: string
      models?: Array<{ id?: string; compat?: Record<string, unknown> }>
    }

    expect(bridge?.modelPattern).toBe('tezbar/moonshotai/kimi-k3-free')
    expect(provider).toMatchObject({
      api: 'openai-completions',
      authHeader: true,
      baseUrl: 'https://api.tokenrouter.com/v1',
    })
    expect(provider.models?.[0]).toMatchObject({
      id: 'moonshotai/kimi-k3-free',
      compat: {
        supportsStore: false,
        maxTokensField: 'max_tokens',
      },
    })
  })

  it('uses DeepSeek Anthropic messages so DSML tool calls stay structured', () => {
    vi.mocked(readRawConfig).mockReturnValue({
      provider: 'deepseek',
      providerConfigs: {
        deepseek: { baseURL: 'https://api.deepseek.com', apiKey: 'deepseek-key' },
      },
      providerSelectedModels: {
        deepseek: 'deepseek-v4-flash',
      },
    })

    const bridge = getSelectedPiProviderBridge('chat')
    const provider = JSON.parse(bridge?.providerJson ?? '{}') as {
      api?: string
      baseUrl?: string
    }

    expect(provider).toMatchObject({
      api: 'anthropic-messages',
      baseUrl: 'https://api.deepseek.com/anthropic',
    })
  })

  it('keeps custom DeepSeek endpoints on OpenAI format with V4 compatibility', () => {
    vi.mocked(readRawConfig).mockReturnValue({
      provider: 'deepseek',
      providerConfigs: {
        deepseek: { baseURL: 'https://deepseek-proxy.example/v1', apiKey: 'proxy-key' },
      },
      providerSelectedModels: {
        deepseek: 'deepseek-v4-flash',
      },
    })

    const bridge = getSelectedPiProviderBridge('chat')
    const provider = JSON.parse(bridge?.providerJson ?? '{}') as {
      api?: string
      baseUrl?: string
      models?: Array<{ compat?: Record<string, unknown> }>
    }

    expect(provider).toMatchObject({
      api: 'openai-completions',
      baseUrl: 'https://deepseek-proxy.example/v1',
    })
    expect(provider.models?.[0]?.compat).toMatchObject({
      supportsStore: false,
      supportsDeveloperRole: false,
      requiresReasoningContentOnAssistantMessages: true,
      thinkingFormat: 'deepseek',
    })
  })
})
