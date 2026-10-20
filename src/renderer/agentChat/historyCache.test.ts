import { afterEach, describe, expect, it } from 'vitest'
import type { ChatSessionSummary } from '../../shared/chat'
import {
  clearCachedChatHistory,
  readCachedChatHistory,
  writeCachedChatHistory,
} from './historyCache'

afterEach(() => {
  clearCachedChatHistory()
})

function summary(id: string, title: string): ChatSessionSummary {
  return {
    id,
    title,
    createdAt: 1,
    updatedAt: 2,
    turnCount: 2,
    preview: 'preview',
  }
}

describe('chat history cache', () => {
  it('starts empty so the first launch still shows a loading state', () => {
    expect(readCachedChatHistory()).toBeNull()
  })

  it('hands the last known list to a remounted launcher', () => {
    writeCachedChatHistory([summary('a', 'First')])

    expect(readCachedChatHistory()).toEqual([summary('a', 'First')])

    writeCachedChatHistory([summary('b', 'Second'), summary('a', 'First')])
    expect(readCachedChatHistory()?.map((row) => row.id)).toEqual(['b', 'a'])
  })

  it('keeps an empty result distinct from never having loaded', () => {
    writeCachedChatHistory([])
    expect(readCachedChatHistory()).toEqual([])
  })
})
