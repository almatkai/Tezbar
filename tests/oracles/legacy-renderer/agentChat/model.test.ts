import { describe, expect, it } from 'vitest'
import { historyPlaceholder } from './model'

describe('agent chat history placeholder', () => {
  it('distinguishes a slow backend from an empty history', () => {
    expect(historyPlaceholder(true)).toBe('Loading conversations…')
    expect(historyPlaceholder(false)).toBe('No saved conversations yet.')
  })
})
