import { describe, expect, it } from 'vitest'
import { StringCache } from './stringCache'

describe('StringCache', () => {
  it('evicts least recently used icons, including cached misses', () => {
    const cache = new StringCache(2)
    cache.set('a', null).set('b', 'icon')
    expect(cache.get('a')).toBeNull()
    cache.set('c', 'icon')
    expect([...cache.keys()]).toEqual(['a', 'c'])
  })

  it('bounds retained bytes and accounts for replacement, deletion and clear', () => {
    const cache = new StringCache(10, 20)
    cache.set('a', '1234').set('b', '1234')
    cache.set('a', '12345678')
    expect([...cache.keys()]).toEqual(['a'])
    cache.set('a', 'too large to retain')
    expect(cache.size).toBe(0)
    cache.set('b', '1234')
    cache.delete('b')
    cache.set('c', '1234').set('d', '1234')
    expect(cache.size).toBe(2)
    cache.clear()
    cache.set('e', '1234').set('f', '1234')
    expect(cache.size).toBe(2)
  })
})
