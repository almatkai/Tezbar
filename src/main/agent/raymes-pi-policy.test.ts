import { afterEach, describe, expect, it } from 'vitest'

import raymesPiPolicy, {
  isAutoAllowedBash,
  isDestructiveBash,
  isPersistentlyAllowedBash,
  isSafeProjectBash,
  preferredIndexedSearchForBash,
} from './raymes-pi-policy'

afterEach(() => {
  delete process.env.TEZBAR_KNOWLEDGE_ENDPOINT
  delete process.env.TEZBAR_KNOWLEDGE_TOKEN
  delete process.env.TOKENROUTER_API_KEY
  delete process.env.RAYMES_PI_PROVIDER_JSON
  delete process.env.RAYMES_PI_APPROVAL_MODE
})

describe('Raymes Pi command policy', () => {
  it('does not treat background commands as an approved command family', () => {
    expect(isAutoAllowedBash('ps & touch marker.txt', new Set(), new Set())).toBe(false)
    expect(
      isPersistentlyAllowedBash('grep needle file & touch marker.txt', new Set(['grep']))
    ).toBe(false)
  })

  it('checks every command before automatically allowing project work', () => {
    for (const command of [
      'echo hello; touch marker.txt',
      'git status && touch marker.txt',
      'cat file | sh',
      'echo $(touch marker.txt)',
      'echo hello\ntouch marker.txt',
      'echo hello > marker.txt',
    ])
      expect(isSafeProjectBash(command)).toBe(false)
    expect(isSafeProjectBash('git diff && cargo check')).toBe(true)
    expect(isAutoAllowedBash('cd project && ls', new Set(), new Set())).toBe(true)
  })

  it('asks for compound commands in Default and Auto modes', async () => {
    let handler!: (
      event: { toolName: string; input: { command: string } },
      ctx: {
        ui: { confirm: () => Promise<boolean> }
      }
    ) => unknown
    raymesPiPolicy({
      registerProvider() {},
      registerTool() {},
      on(_event, callback) {
        handler = callback
      },
    } as Parameters<typeof raymesPiPolicy>[0])
    for (const [mode, command] of [
      ['default', 'ps & touch marker.txt'],
      ['auto', 'echo hello; touch marker.txt'],
      ['auto', 'git status && touch marker.txt'],
    ]) {
      process.env.RAYMES_PI_APPROVAL_MODE = mode
      let confirmations = 0
      const result = await handler(
        { toolName: 'bash', input: { command } },
        {
          ui: {
            confirm: async () => {
              confirmations++
              return false
            },
          },
        }
      )
      expect(confirmations).toBe(1)
      expect(result).toMatchObject({ block: true })
    }
  })

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

  it('registers TokenRouter as an OpenAI-compatible Pi provider', () => {
    const providers: Array<{ name: string; config: Record<string, unknown> }> = []
    const pi = {
      registerProvider: (name: string, config: Record<string, unknown>) => {
        providers.push({ name, config })
      },
      registerTool: () => {},
      on: () => {},
    } as unknown as Parameters<typeof raymesPiPolicy>[0]

    raymesPiPolicy(pi)

    const tokenrouter = providers.find((provider) => provider.name === 'tokenrouter')
    expect(tokenrouter?.config).toMatchObject({
      baseUrl: 'https://api.tokenrouter.com/v1',
      apiKey: '$TOKENROUTER_API_KEY',
      authHeader: true,
      api: 'openai-completions',
    })
    expect(tokenrouter?.config.models).toEqual([
      expect.objectContaining({
        id: 'moonshotai/kimi-k3-free',
        reasoning: false,
      }),
    ])
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

  it('correctly identifies destructive commands that could wipe data or the system', () => {
    // Massive filesystem wipes
    expect(isDestructiveBash('rm -rf /')).toBe(true)
    expect(isDestructiveBash('rm -rf /*')).toBe(true)
    expect(isDestructiveBash('rm -rf ~')).toBe(true)
    expect(isDestructiveBash('rm -rf $HOME')).toBe(true)
    expect(isDestructiveBash('rm -rf /Users')).toBe(true)
    expect(isDestructiveBash('rm -rf /System')).toBe(true)
    expect(isDestructiveBash('rm -rf /Library')).toBe(true)
    expect(isDestructiveBash('rm -rf *')).toBe(true)
    expect(isDestructiveBash('rd /s /q C:\\')).toBe(true)

    // Database deletion / drops
    expect(isDestructiveBash('DROP DATABASE production;')).toBe(true)
    expect(isDestructiveBash('DROP TABLE users;')).toBe(true)
    expect(isDestructiveBash('TRUNCATE TABLE logs;')).toBe(true)
    expect(isDestructiveBash('rm -f data.sqlite')).toBe(true)
    expect(isDestructiveBash('rm -rf ~/.tezbar')).toBe(true)

    // Disk formatting / raw writing
    expect(isDestructiveBash('mkfs.ext4 /dev/sda1')).toBe(true)
    expect(isDestructiveBash('diskutil eraseDisk APFS MyDisk /dev/disk2')).toBe(true)
    expect(isDestructiveBash('dd if=/dev/zero of=/dev/rdisk0')).toBe(true)

    // System disruption
    expect(isDestructiveBash(':(){ :|:& };:')).toBe(true)
    expect(isDestructiveBash('shutdown -h now')).toBe(true)
    expect(isDestructiveBash('reboot')).toBe(true)
    expect(isDestructiveBash('chmod -R 000 /')).toBe(true)

    // Safe commands
    expect(isDestructiveBash('git status')).toBe(false)
    expect(isDestructiveBash('npm test')).toBe(false)
    expect(isDestructiveBash('rm -rf target/debug')).toBe(false)
    expect(isDestructiveBash('rm -rf node_modules')).toBe(false)
    expect(isDestructiveBash('mkdir -p build')).toBe(false)
  })

  it('respects approvalMode settings (review, default, auto, turbo)', async () => {
    let handler:
      | ((
          event: { toolName: string; input?: { command?: unknown } },
          ctx: { ui: { confirm: (title: string, cmd: string) => Promise<boolean> } }
        ) => unknown)
      | undefined
    const pi = {
      registerProvider: () => {},
      registerTool: () => {},
      on: (_event: string, callback: typeof handler) => {
        handler = callback
      },
    } as unknown as Parameters<typeof raymesPiPolicy>[0]

    let confirmCalls: string[] = []
    const confirm = async (_title: string, cmd: string) => {
      confirmCalls.push(cmd)
      return true
    }

    // Mode: turbo (auto-allows non-destructive commands, asks on destructive)
    process.env.RAYMES_PI_APPROVAL_MODE = 'turbo'
    confirmCalls = []
    raymesPiPolicy(pi)

    const safeRes = await handler?.(
      { toolName: 'bash', input: { command: 'cargo build' } },
      { ui: { confirm } }
    )
    expect(safeRes).toBeUndefined()
    expect(confirmCalls).toHaveLength(0)

    const destructiveRes = await handler?.(
      { toolName: 'bash', input: { command: 'rm -rf /' } },
      { ui: { confirm } }
    )
    expect(destructiveRes).toBeUndefined()
    expect(confirmCalls).toContain('rm -rf /')

    // Mode: review (always asks, even for safe commands)
    process.env.RAYMES_PI_APPROVAL_MODE = 'review'
    confirmCalls = []
    const reviewRes = await handler?.(
      { toolName: 'bash', input: { command: 'pwd' } },
      { ui: { confirm } }
    )
    expect(reviewRes).toBeUndefined()
    expect(confirmCalls).toContain('pwd')

    delete process.env.RAYMES_PI_APPROVAL_MODE
  })

  it('allows safe inspection commands like top without prompting in turbo mode', async () => {
    let handler:
      | ((
          event: { toolName: string; input?: { command?: unknown } },
          ctx: { ui: { confirm: (title: string, cmd: string) => Promise<boolean> } }
        ) => unknown)
      | undefined
    const pi = {
      registerProvider: () => {},
      registerTool: () => {},
      on: (_event: string, callback: typeof handler) => {
        handler = callback
      },
    } as unknown as Parameters<typeof raymesPiPolicy>[0]

    let confirmCalls: string[] = []
    const confirm = async (_title: string, cmd: string) => {
      confirmCalls.push(cmd)
      return true
    }

    process.env.RAYMES_PI_APPROVAL_MODE = 'turbo'
    confirmCalls = []
    raymesPiPolicy(pi)

    const topRes = await handler?.(
      { toolName: 'bash', input: { command: 'top -l 2 -s 2 -n 10 -o cpu' } },
      { ui: { confirm } }
    )
    expect(topRes).toBeUndefined()
    expect(confirmCalls).toHaveLength(0)

    delete process.env.RAYMES_PI_APPROVAL_MODE
  })
})
