import { afterEach, describe, expect, it, vi } from 'vitest'
import { copyVaultSecret } from './vaultCopy'

function stubVault(unlocked: boolean, unlockResult = { ok: true }) {
  const api = {
    vaultStatus: vi.fn().mockResolvedValue({ supported: true, unlocked, itemCount: 1 }),
    vaultUnlock: vi.fn().mockResolvedValue(unlockResult),
    vaultCopy: vi.fn().mockResolvedValue({ ok: true }),
  }
  vi.stubGlobal('window', { tezbar: api })
  return api
}

afterEach(() => vi.unstubAllGlobals())

describe('copyVaultSecret', () => {
  it('copies straight away while the vault is open', async () => {
    const api = stubVault(true)
    expect(await copyVaultSecret('a1')).toBe('Copied · clears in 30 s')
    expect(api.vaultUnlock).not.toHaveBeenCalled()
    expect(api.vaultCopy).toHaveBeenCalledWith('a1')
  })

  it('asks for Touch ID first when locked', async () => {
    const api = stubVault(false)
    expect(await copyVaultSecret('a1')).toBe('Copied · clears in 30 s')
    expect(api.vaultUnlock).toHaveBeenCalledOnce()
    expect(api.vaultCopy).toHaveBeenCalledWith('a1')
  })

  it('copies nothing when the prompt is cancelled', async () => {
    const api = stubVault(false, { ok: false })
    expect(await copyVaultSecret('a1')).toBeNull()
    expect(api.vaultCopy).not.toHaveBeenCalled()
  })

  it('reports errors as a message', async () => {
    const api = stubVault(true)
    api.vaultCopy.mockRejectedValue(new Error('This secret no longer exists'))
    expect(await copyVaultSecret('gone')).toBe('This secret no longer exists')
  })
})
