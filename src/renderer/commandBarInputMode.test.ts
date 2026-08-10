import { describe, expect, it } from 'vitest'
import { aiChatBootForInput, commandBarInputMode } from './commandBarInputMode'

describe('command bar input mode', () => {
  it('gives AI mode priority over slash-path completion after double Space', () => {
    const mode = commandBarInputMode('/Desktop/code/aml  ', false)

    expect(mode.isAiMode).toBe(true)
    expect(mode.isCompletionInput).toBe(false)
    expect(mode.aiWorkingDirectory).toBe('/Desktop/code/aml')
    expect(mode.aiTask).toBe('')
    expect(aiChatBootForInput(mode, '/Users/dev/Desktop/code/aml')).toEqual({
      kind: 'newChat',
      workingDirectory: '/Users/dev/Desktop/code/aml',
    })
  })

  it('keeps AI mode active and searches chats after typing beyond the double-Space switch', () => {
    const mode = commandBarInputMode('/Desktop/code/aml  explain this project', false)

    expect(mode.isAiMode).toBe(true)
    expect(mode.isCompletionInput).toBe(false)
    expect(mode.aiWorkingDirectory).toBe('/Desktop/code/aml')
    expect(mode.aiTask).toBe('explain this project')
    expect(aiChatBootForInput(mode, '/Users/dev/Desktop/code/aml')).toEqual({
      kind: 'submit',
      prompt: 'explain this project',
      workingDirectory: '/Users/dev/Desktop/code/aml',
    })
  })

  it('preserves regular AI activation when two spaces are trailing', () => {
    const mode = commandBarInputMode('search  ', false)

    expect(mode.isAiMode).toBe(true)
    expect(mode.aiTask).toBe('search')
  })

  it('does not treat an ordinary internal double-space query as AI mode', () => {
    const mode = commandBarInputMode('search  two words', false)

    expect(mode.isAiMode).toBe(false)
    expect(mode.isCompletionInput).toBe(false)
  })

  it('preserves leading-Space AI prompts without treating them as directories', () => {
    const mode = commandBarInputMode(' explain this project', false)

    expect(mode.isAiMode).toBe(true)
    expect(mode.aiWorkingDirectory).toBeUndefined()
    expect(mode.aiTask).toBe('explain this project')
  })
})
