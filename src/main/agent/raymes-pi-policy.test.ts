import { afterEach, describe, expect, it } from 'vitest'

import raymesPiPolicy, {
  isAutoAllowedBash,
  isPersistentlyAllowedBash,
  preferredIndexedSearchForBash,
} from './raymes-pi-policy'

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
    expect(isPersistentlyAllowedBash('grep $(rm -rf /tmp/example)', new Set(['grep']))).toBe(false)
  })

  it('uses persisted rules alongside the built-in safe commands', () => {
    expect(isAutoAllowedBash('grep -R "needle" src', new Set(['grep']))).toBe(true)
    expect(isAutoAllowedBash('git status', new Set())).toBe(true)
    expect(isAutoAllowedBash('rm -rf build', new Set(['grep']))).toBe(false)
  })

  it('allows an explicitly remembered exact command without broadening the rule', () => {
    const remembered = 'rg -l "needle" ~/code 2>/dev/null | head -30'
    expect(isAutoAllowedBash(remembered, new Set(), new Set([remembered]))).toBe(true)
    expect(
      isAutoAllowedBash(
        'rg -l "different" ~/code 2>/dev/null | head -30',
        new Set(),
        new Set([remembered])
      )
    ).toBe(false)
  })

  it('routes broad personal-file scans to the appropriate Tezbar index', () => {
    expect(
      preferredIndexedSearchForBash('grep -rl "No active mortgages found" ~ 2>/dev/null | head -30')
    ).toBe('deep')
    expect(
      preferredIndexedSearchForBash(
        'find ~/Desktop ~/Documents ~/Downloads ~/code -type f -name "*.txt"'
      )
    ).toBe('launcher')
    expect(preferredIndexedSearchForBash('rg "pc_search" src/main/agent')).toBeNull()
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

    expect(tools).toContainEqual(expect.objectContaining({ name: 'launcher_search' }))
    expect(tools).toContainEqual(expect.objectContaining({ name: 'pc_search' }))
    expect(tools).toContainEqual(expect.objectContaining({ name: 'pc_read' }))
  })

  it('blocks a broad grep until Deep Search has been attempted', async () => {
    process.env.TEZBAR_KNOWLEDGE_ENDPOINT = 'http://127.0.0.1:43123/search'
    process.env.TEZBAR_KNOWLEDGE_TOKEN = 'test-token'
    let handler:
      | ((
          event: { toolName: string; input?: { command?: unknown } },
          ctx: {
            ui: { confirm: () => Promise<boolean> }
          }
        ) => unknown)
      | undefined
    const pi = {
      registerProvider: () => {},
      registerTool: () => {},
      on: (_event: string, callback: typeof handler) => {
        handler = callback
      },
    } as unknown as Parameters<typeof raymesPiPolicy>[0]
    const confirm = async () => true

    raymesPiPolicy(pi)

    await expect(
      handler?.(
        { toolName: 'bash', input: { command: 'grep -rl "needle" ~ 2>/dev/null' } },
        { ui: { confirm } }
      )
    ).resolves.toEqual(expect.objectContaining({ block: true }))

    await handler?.({ toolName: 'pc_search', input: {} }, { ui: { confirm } })
    await expect(
      handler?.(
        { toolName: 'bash', input: { command: 'grep -rl "needle" ~ 2>/dev/null' } },
        { ui: { confirm } }
      )
    ).resolves.toBeUndefined()
  })
})
