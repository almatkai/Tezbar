import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import type { VaultKind } from '../shared/vault'
import { VAULT_DEFAULT_ICONS, VAULT_ICONS, VaultItemIcon, resolveVaultIcon } from './vaultIcons'

describe('vault icons', () => {
  it('keeps persisted ids unique and within the Rust validation format', () => {
    const ids = VAULT_ICONS.map((entry) => entry.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const id of ids) expect(id).toMatch(/^[a-z0-9-]{1,32}$/)
  })

  it('has a registered default for every VaultKind', () => {
    const kinds: VaultKind[] = ['api-key', 'token', 'ssh-key', 'server', 'password', 'note']
    expect(Object.keys(VAULT_DEFAULT_ICONS).sort()).toEqual([...kinds].sort())
    for (const kind of kinds) {
      expect(VAULT_ICONS.some((entry) => entry.id === VAULT_DEFAULT_ICONS[kind])).toBe(true)
      expect(resolveVaultIcon(kind).id).toBe(VAULT_DEFAULT_ICONS[kind])
    }
  })

  it('renders a custom icon and falls back to the kind for an unknown id', () => {
    const render = (icon?: string): string =>
      renderToStaticMarkup(createElement(VaultItemIcon, { kind: 'server', icon }))
    expect(resolveVaultIcon('server', 'cloud').id).toBe('cloud')
    expect(render('cloud')).not.toBe(render())
    expect(render('future-icon')).toBe(render())
    expect(render()).toContain('stroke-width="1.2"')
    expect(render()).toContain('viewBox="0 0 14 14"')
    expect(render()).toContain('h-7 w-7')
  })
})
