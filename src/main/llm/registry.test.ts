import { describe, expect, it, vi } from 'vitest'
import { readRawConfig } from './configStore'
import { configForProvider, readLLMConfig, type OpenRayLLMConfig } from './registry'

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
})
