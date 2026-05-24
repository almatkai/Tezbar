import { describe, expect, it } from 'vitest'
import { buildFtsQuery } from './textMatch'

describe('buildFtsQuery', () => {
  it('uses only FTS-safe prefix tokens', () => {
    expect(buildFtsQuery('port-manager')).toBe('port* OR manager*')
    expect(buildFtsQuery('raycast.port_manager')).toBe('raycast* OR port* OR manager*')
  })

  it('drops punctuation-only queries', () => {
    expect(buildFtsQuery('*')).toBe('')
    expect(buildFtsQuery('--- ... ___')).toBe('')
  })
})
