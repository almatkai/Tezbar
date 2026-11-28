import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  INITIAL_DEFAULT_TARGET,
  __setCurrencyPrefsStorage,
  getPinnedDefault,
  getPreferredDefaultTarget,
  listTargetUsage,
  recordTargetUsage,
  setPinnedDefault,
} from './currencyPreferences'

function makeMemory(): { getItem: (k: string) => string | null; setItem: (k: string, v: string) => void; removeItem: (k: string) => void } {
  const store = new Map<string, string>()
  return {
    getItem: (k) => (store.has(k) ? store.get(k)! : null),
    setItem: (k, v) => {
      store.set(k, v)
    },
    removeItem: (k) => {
      store.delete(k)
    },
  }
}

describe('currencyPreferences', () => {
  beforeEach(() => {
    __setCurrencyPrefsStorage(makeMemory())
  })
  afterEach(() => {
    __setCurrencyPrefsStorage(null)
  })

  it('starts with the initial default and no usage', () => {
    expect(getPreferredDefaultTarget()).toBe(INITIAL_DEFAULT_TARGET)
    expect(listTargetUsage()).toEqual([])
    expect(getPinnedDefault()).toBeNull()
  })

  it('ranks targets by usage frequency', () => {
    recordTargetUsage('KZT')
    recordTargetUsage('KZT')
    recordTargetUsage('EUR')
    recordTargetUsage('KZT')
    expect(getPreferredDefaultTarget()).toBe('KZT')
    const ranked = listTargetUsage()
    expect(ranked[0]).toMatchObject({ code: 'KZT', count: 3 })
    expect(ranked[1]).toMatchObject({ code: 'EUR', count: 1 })
  })

  it('pinned override wins over usage', () => {
    recordTargetUsage('KZT')
    recordTargetUsage('KZT')
    setPinnedDefault('USD')
    expect(getPreferredDefaultTarget()).toBe('USD')
    setPinnedDefault(null)
    expect(getPreferredDefaultTarget()).toBe('KZT')
  })

  it('ignores invalid codes', () => {
    recordTargetUsage('zz')
    recordTargetUsage('123')
    setPinnedDefault('!!')
    expect(listTargetUsage()).toEqual([])
    expect(getPinnedDefault()).toBeNull()
  })
})
