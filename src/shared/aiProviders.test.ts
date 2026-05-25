import { describe, expect, it } from 'vitest'
import { normalizeProviderModelList } from './aiProviders'
import type { AiProviderModel } from './llmConfig'

describe('normalizeProviderModelList', () => {
  it('removes legacy context capability while preserving context window size', () => {
    const models = normalizeProviderModelList('deepseek', [
      {
        id: 'deepseek-v4-flash',
        capabilities: ['tools', 'context'] as unknown as AiProviderModel['capabilities'],
        contextWindow: 128000,
      },
    ])

    expect(models[0]).toEqual({
      id: 'deepseek-v4-flash',
      capabilities: ['tools'],
      contextWindow: 128000,
    })
  })
})
