import { describe, expect, it } from 'vitest'
import { getInstalledPiExtensions } from './piExtensions'

describe('getInstalledPiExtensions', () => {
  it('discovers installed Pi packages and extensions', () => {
    const list = getInstalledPiExtensions()
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
