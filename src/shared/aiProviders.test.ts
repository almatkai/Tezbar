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

  it('keeps hand-added models whose id matches a sibling provider default', () => {
    // Regression: a user-added Copilot model named `auto` (or any id listed as a
    // default under another provider, e.g. `claude-opus-4.5` under anthropic)
    // was previously stripped by the cross-provider "sibling defaults" filter,
    // so it saved in Settings but never appeared in the AI-mode picker.
    const models = normalizeProviderModelList('copilot', [
      { id: 'gpt-5-mini', capabilities: ['vision', 'thinking', 'tools'] },
      { id: 'auto', capabilities: ['tools', 'vision', 'thinking'] },
      { id: 'claude-opus-4.5', capabilities: ['vision', 'thinking', 'tools'] },
      { id: 'gpt-4.1', capabilities: ['tools'] },
    ])

    expect(models.map((m) => m.id)).toEqual([
      'gpt-5-mini',
      'auto',
      'claude-opus-4.5',
      'gpt-4.1',
    ])
  })

  it('does not resurrect a recommended model the user removed', () => {
    const models = normalizeProviderModelList('copilot', [])
    expect(models).toEqual([])
  })

  it('dedupes and drops blank ids', () => {
    const models = normalizeProviderModelList('copilot', [
      { id: 'gpt-4o', capabilities: ['vision', 'tools'] },
      { id: 'gpt-4o', capabilities: ['vision', 'tools'] },
      { id: '   ', capabilities: [] },
    ])
    expect(models.map((m) => m.id)).toEqual(['gpt-4o'])
  })
})
