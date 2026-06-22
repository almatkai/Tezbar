import { describe, expect, it } from 'vitest'

import { isAutoAllowedBash, isPersistentlyAllowedBash } from './raymes-pi-policy'

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
})
