import { afterEach, describe, expect, it } from 'vitest'
import {
  getManifestPlatforms,
  isCommandPlatformCompatible,
  isManifestPlatformCompatible,
} from './extension-platform'
const descriptor = Object.getOwnPropertyDescriptor(process, 'platform')!
afterEach(() => Object.defineProperty(process, 'platform', descriptor))
function platform(value: string) {
  Object.defineProperty(process, 'platform', { ...descriptor, value })
}

describe('extension platform eligibility', () => {
  it('defaults legacy undeclared extensions to macOS instead of pretending Windows support', () => {
    expect(getManifestPlatforms({ name: 'timers' })).toEqual(['macOS'])
    platform('win32')
    expect(isManifestPlatformCompatible({ name: 'timers' })).toBe(false)
    platform('darwin')
    expect(isManifestPlatformCompatible({ name: 'timers' })).toBe(true)
  })
  it('accepts explicit Windows declarations and Tezbar metadata', () => {
    platform('win32')
    expect(isManifestPlatformCompatible({ platforms: ['macOS', 'Windows'] })).toBe(true)
    expect(isManifestPlatformCompatible({ tezbar: { platforms: ['windows', 'macos'] } })).toBe(true)
  })
  it('does not let empty/invalid declarations bypass filtering', () => {
    platform('win32')
    expect(isManifestPlatformCompatible({ platforms: [] })).toBe(false)
    expect(isManifestPlatformCompatible({ platforms: ['unknown'] })).toBe(false)
    expect(isManifestPlatformCompatible(null)).toBe(false)
  })
  it('lets undeclared commands inherit the extension while hiding macOS-only commands on Windows', () => {
    platform('win32')
    expect(isCommandPlatformCompatible({ name: 'index' })).toBe(true)
    expect(isCommandPlatformCompatible({ name: 'menu', platforms: ['macOS'] })).toBe(false)
  })
})
