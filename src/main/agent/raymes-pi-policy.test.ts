import { afterEach, describe, expect, it } from 'vitest'

import raymesPiPolicy, { isAutoAllowedBash, isPersistentlyAllowedBash } from './raymes-pi-policy'

afterEach(() => {
  delete process.env.TEZBAR_KNOWLEDGE_ENDPOINT
  delete process.env.TEZBAR_KNOWLEDGE_TOKEN
})

describe('Raymes Pi command policy', () => {
  it('allows an approved read-only command family in a safe pipeline', () => {
    expect(
      isPersistentlyAllowedBash('ps aux | grep -i "tezbar" | grep -v grep', new Set(['grep']))
    ).toBe(true)
  })

  it('keeps the persisted rule scoped to that command family', () => {
    expect(isPersistentlyAllowedBash('grep foo | rm -rf /tmp/example', new Set(['grep']))).toBe(
      false
    )
    expect(isPersistentlyAllowedBash('grep $(rm -rf /tmp/example)', new Set(['grep']))).toBe(
      false
    )
  })

  it('uses persisted rules alongside the built-in safe commands', () => {
    expect(isAutoAllowedBash('grep -R "needle" src', new Set(['grep']))).toBe(true)
    expect(isAutoAllowedBash('git status', new Set())).toBe(true)
    expect(isAutoAllowedBash('rm -rf build', new Set(['grep']))).toBe(false)
  })

  it('registers the read-only knowledge tool only for an authenticated loopback bridge', () => {
    process.env.TEZBAR_KNOWLEDGE_ENDPOINT = 'http://127.0.0.1:43123/search'
    process.env.TEZBAR_KNOWLEDGE_TOKEN = 'test-token'
    const tools: Array<{ name: string }> = []
    const pi = {
      registerProvider: () => {},
      registerTool: (definition: { name: string }) => tools.push(definition),
      on: () => {},
    } as unknown as Parameters<typeof raymesPiPolicy>[0]

    raymesPiPolicy(pi)

    expect(tools).toContainEqual(expect.objectContaining({ name: 'pc_search' }))
    expect(tools).toContainEqual(expect.objectContaining({ name: 'pc_read' }))
  })
})
