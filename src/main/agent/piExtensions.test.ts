import { describe, expect, it } from 'vitest'
import { getInstalledPiExtensions } from './piExtensions'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

describe('getInstalledPiExtensions', () => {
  it('discovers installed Pi packages and extensions', () => {
    const agentDir = mkdtempSync(join(tmpdir(), 'tezbar-pi-extensions-'))
    let list: ReturnType<typeof getInstalledPiExtensions>
    try {
      writeFileSync(
        join(agentDir, 'settings.json'),
        JSON.stringify({ packages: ['npm:pi-antigravity', 'npm:pi-agents'] })
      )
      const packageDir = join(agentDir, 'npm', 'node_modules', 'pi-antigravity')
      mkdirSync(packageDir, { recursive: true })
      writeFileSync(
        join(packageDir, 'package.json'),
        JSON.stringify({ name: 'pi-antigravity', version: '1.0.0' })
      )
      list = getInstalledPiExtensions(agentDir)
    } finally {
      rmSync(agentDir, { recursive: true, force: true })
    }
    expect(Array.isArray(list)).toBe(true)
    expect(list.length).toBeGreaterThan(0)

    const ids = list.map((item) => item.id)
    expect(ids).toContain('pi-antigravity')
    expect(ids).toContain('pi-agents')

    const antigravity = list.find((item) => item.id === 'pi-antigravity')
    expect(antigravity?.name).toBe('pi-antigravity')
    expect(antigravity?.enabled).toBe(true)
    expect(antigravity?.features).toContain('Google Cloud Code Provider')

    const agents = list.find((item) => item.id === 'pi-agents')
    expect(agents?.features).toContain('Multi-agent orchestration')
  })
})
