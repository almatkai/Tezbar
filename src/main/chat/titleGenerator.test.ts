import { describe, expect, it, vi } from 'vitest'
import { generateChatTitleAndIcon } from './titleGenerator'

vi.mock('../llm/registry', () => ({
  getProviderForTask: vi.fn(),
}))

describe('generateChatTitleAndIcon', () => {
  it('falls back to heuristic when no LLM provider is available', async () => {
    const meta = await generateChatTitleAndIcon('What is using the most CPU?')
    expect(meta.icon).toBe('cpu')
    expect(meta.title).toBe('CPU & Process Diagnostics')
  })

  it('parses LLM JSON response for title and emoji icon', async () => {
    const { getProviderForTask } = await import('../llm/registry')
    const mockProvider = {
      isAvailable: vi.fn().mockResolvedValue(true),
      chat: vi.fn().mockResolvedValue((async function* () {
        yield { text: '{"title": "High CPU Debugging", "icon": "💻"}' }
      })()),
    }
    vi.mocked(getProviderForTask).mockReturnValue(mockProvider as any)

    const meta = await generateChatTitleAndIcon('some complex query')
    expect(meta.title).toBe('High CPU Debugging')
    expect(meta.icon).toBe('💻')
  })
})
