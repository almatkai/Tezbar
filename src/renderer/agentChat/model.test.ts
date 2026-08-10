import { describe, expect, it } from 'vitest'
import type { ChatSession } from '../../shared/chat'
import { buildAgentPromptFromChat } from './model'

describe('agent chat prompt', () => {
  it('keeps a directory-rooted Pi chat inside its working directory by default', () => {
    const session: ChatSession = {
      id: 'chat-1',
      title: 'Project chat',
      createdAt: 1,
      updatedAt: 1,
      workingDirectory: '/Users/dev/Desktop/code/aml',
      turns: [],
    }

    const prompt = buildAgentPromptFromChat(session, 'Explain the architecture')

    expect(prompt).toContain('Your working directory is /Users/dev/Desktop/code/aml.')
    expect(prompt).toContain('Stay inside this directory unless the user explicitly asks')
  })
})
