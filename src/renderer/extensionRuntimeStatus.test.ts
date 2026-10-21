import { describe, expect, it } from 'vitest'
import type { ExtensionManifest, InstalledExtension, NativeV2Info } from '../shared/extensions'
import {
  checkedLines,
  canInstallExtension,
  commandStatus,
  describeRuntime,
  effectiveRuntime,
  installTarget,
  mergeCatalog,
  replacementReason,
  rowSubtitle,
  searchStorePage,
  visibleCommands,
} from './extensionRuntimeStatus'

const item = (id: string, extra: Partial<ExtensionManifest> = {}): ExtensionManifest => ({
  id, name: id, description: '', author: 'Tezbar', version: '1', ...extra,
})
const native = (extra: Partial<NativeV2Info> = {}): NativeV2Info => ({
  schemaVersion: 2,
  id: 'com.tezbar.native.foo',
  version: '2.1.0',
  replaces: ['raycast.foo'],
  commands: [{ name: 'open', runtime: 'rust', platforms: ['macOS', 'Windows'] }],
  totalCommands: 1,
  status: 'complete',
  verification: [],
  ...extra,
})

describe('extension runtime status', () => {
  it('downgrades an incomplete Rust claim and counts only current-platform commands', () => {
    const commands = ['a', 'b', 'c'].map((name) => ({
      name, runtime: 'rust' as const, platforms: ['macOS' as const],
    }))
    const ext = item('raycast.foo', { runtime: 'rust', nativeV2: native({
      commands: [...commands, { name: 'd', runtime: 'javascript', platforms: ['macOS'] }],
      totalCommands: 4,
      status: 'partial',
    }) })
    expect(effectiveRuntime(ext, 'macOS')).toBe('mixed')
    expect(describeRuntime(ext, 'macOS')?.short).toBe('Rust 3 of 4')
  })

  it('does not infer Rust without command metadata or any runtime without metadata', () => {
    expect(effectiveRuntime(item('raycast.foo', { runtime: 'rust' }), 'macOS')).toBeNull()
    expect(effectiveRuntime(item('raycast.foo'), 'macOS')).toBeNull()
    expect(rowSubtitle(item('raycast.foo'), 'macOS', 'Installed')).toBe('Installed')
  })

  it('marks a macOS-only Rust command unavailable on Windows', () => {
    const command = { name: 'open', runtime: 'rust' as const, platforms: ['macOS' as const] }
    const ext = item('raycast.foo', { nativeV2: native({ commands: [command] }) })
    expect(commandStatus(command, 'Windows')).toBe('Not on Windows')
    expect(effectiveRuntime(ext, 'Windows')).toBe('javascript')
    expect(describeRuntime(ext, 'Windows')?.short).toBe('Rust 0 of 1')
    expect(describeRuntime(ext, 'Windows')?.detail).toBe('No commands available on Windows')
  })

  it('requires dated checks for passed and failed states, independently per platform', () => {
    const ext = item('raycast.foo', { nativeV2: native({ verification: [
      { platform: 'macOS', state: 'passed' },
      { platform: 'Windows', state: 'passed', checkedAt: 1792540800000, method: 'automated' },
    ] }) })
    expect(checkedLines(ext, 'macOS', () => 'Oct 21, 2026')).toEqual([
      'macOS — not checked yet',
      'Windows — passed Oct 21, 2026 (automated)',
    ])
    const macOnly = item('raycast.foo', { nativeV2: native({ verification: [
      { platform: 'macOS', state: 'passed', checkedAt: 1792540800000 },
    ] }) })
    expect(checkedLines(macOnly, 'Windows', () => 'Oct 21, 2026')).toEqual([
      'Windows — not checked yet',
      'macOS — passed Oct 21, 2026',
    ])
    const failed = item('raycast.foo', { nativeV2: native({ verification: [
      { platform: 'macOS', state: 'failed', checkedAt: 1792540800000, note: 'Command timed out' },
    ] }) })
    expect(checkedLines(failed, 'macOS', () => 'Oct 21, 2026')[0]).toBe(
      'macOS — failed Oct 21, 2026: Command timed out',
    )
    expect(rowSubtitle(failed, 'macOS', 'Installed')).toBe('Installed · Rust · check failed')
    const latest = item('raycast.foo', { nativeV2: native({ verification: [
      { platform: 'macOS', state: 'not-run' },
      { platform: 'macOS', state: 'passed', checkedAt: 100 },
      { platform: 'macOS', state: 'failed', checkedAt: 200, note: 'Latest check failed' },
      { platform: 'macOS', state: 'passed' },
    ] }) })
    expect(checkedLines(latest, 'macOS', () => 'Today')[0]).toBe('macOS — failed Today: Latest check failed')
    expect(rowSubtitle(latest, 'macOS', 'Installed')).toContain('check failed')
  })

  it('trusts an installed JavaScript runtime over unrelated native metadata', () => {
    const ext = item('raycast.foo', { runtime: 'javascript', nativeV2: native() })
    expect(effectiveRuntime(ext, 'macOS')).toBe('javascript')
    expect(describeRuntime(ext, 'macOS')?.short).toBe('JavaScript')
  })

  it('disables a native install with no runnable command on this platform', () => {
    const ext = item('com.tezbar.native.foo', { nativeV2: native({ commands: [
      { name: 'open', runtime: 'unavailable', platforms: ['macOS', 'Windows'] },
    ] }) })
    expect(canInstallExtension(ext, 'macOS')).toBe(false)
    expect(replacementReason(ext)).toBe('Tezbar replacement for raycast.foo')
    expect(canInstallExtension(item('raycast.foo'), 'macOS')).toBe(true)
  })

  it('appends commands present only in Native V2 metadata', () => {
    const ext = item('raycast.foo', {
      commands: [{ name: 'old', title: 'Old' }],
      nativeV2: native({ commands: [
        { name: 'old', runtime: 'rust', platforms: ['macOS'] },
        { name: 'new', title: 'New', runtime: 'unavailable', platforms: [] },
      ], totalCommands: 2 }),
    })
    expect(visibleCommands(ext).map((command) => command.name)).toEqual(['old', 'new'])
  })
})

describe('catalog merging', () => {
  const replacement = item('com.tezbar.native.foo', { nativeV2: native() })
  const raycast = item('raycast.foo')

  it('keeps a partial replacement separate from its Raycast twin', () => {
    const rows = mergeCatalog([item('before'), raycast, replacement, item('after')], [], 'macOS')
    expect(rows.map((row) => row.id)).toEqual(['before', 'raycast.foo', 'com.tezbar.native.foo', 'after'])
    expect(rows[1]?.nativeV2).toBeUndefined()
  })

  it('preserves canonical and installed Raycast rows independently on Windows', () => {
    expect(mergeCatalog([replacement], [], 'Windows').map((row) => row.id))
      .toEqual(['com.tezbar.native.foo'])
    const rows = mergeCatalog([
      item('before'), item('raycast.foo', { platforms: ['macOS'] }), item('after'), replacement,
    ], [{ ...raycast, installedAt: 1 }], 'Windows')
    expect(rows.map((row) => row.id)).toEqual([
      'before', 'raycast.foo', 'after', 'com.tezbar.native.foo',
    ])
    expect(installTarget(rows[3]!)).toBe('com.tezbar.native.foo')
  })

  it('overlays installed runtime metadata and keeps installed items absent from store', () => {
    const local: InstalledExtension = {
      ...item('raycast.foo'), installedAt: 1, runtime: 'rust', nativeV2: native(),
      platforms: ['macOS', 'Windows'],
    }
    const missing: InstalledExtension = { ...item('raycast.local'), installedAt: 2 }
    const rows = mergeCatalog([item('raycast.foo', {
      author: 'Store Author', iconUrl: 'https://example.test/icon.png', downloadCount: 42,
    })], [local, missing], 'macOS')
    expect(rows[0]?.runtime).toBe('rust')
    expect(rows[0]?.nativeV2).toEqual(local.nativeV2)
    expect(rows[0]?.platforms).toEqual(['macOS', 'Windows'])
    expect(rows[0]?.author).toBe('Store Author')
    expect(rows[0]?.iconUrl).toBe('https://example.test/icon.png')
    expect(rows[0]?.downloadCount).toBe(42)
    expect(rows.map((row) => row.id)).toEqual(['raycast.foo', 'raycast.local'])
    expect(mergeCatalog([raycast], [local, missing], 'macOS', 'foo').map((row) => row.id))
      .toEqual(['raycast.foo'])
    expect(mergeCatalog([], [missing], 'macOS', 'local').map((row) => row.id))
      .toEqual(['raycast.local'])
  })

  it('does not retain native command labels when installed runtime is JavaScript', () => {
    const rows = mergeCatalog([item('raycast.foo', { nativeV2: native(), runtime: 'rust' })], [
      { ...raycast, installedAt: 1, runtime: 'javascript' },
    ], 'macOS')
    expect(rows[0]?.runtime).toBe('javascript')
    expect(rows[0]?.nativeV2).toBeUndefined()
  })
})

describe('store search', () => {
  it('uses the backend page on Windows as well as macOS', async () => {
    const page = { items: [item('com.tezbar.native.foo', { nativeV2: native() })],
      total: 1, offset: 0, limit: 25, hasMore: false }
    const calls: Array<{ query: string; offset: number; limit: number }> = []
    const search = async (query: string, options: { offset: number; limit: number }) => {
      calls.push({ query, ...options })
      return page
    }
    expect(await searchStorePage('foo', false, 0, 25, search)).toEqual(page)
    expect(calls).toEqual([{ query: 'foo', offset: 0, limit: 25 }])
    expect(await searchStorePage('https://github.com/a/b', true, 0, 25, search)).toEqual({
      items: [], total: 0, offset: 0, limit: 25, hasMore: false,
    })
    expect(calls).toHaveLength(1)
  })
})
