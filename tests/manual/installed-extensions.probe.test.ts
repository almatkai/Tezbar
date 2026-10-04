/* eslint-disable @typescript-eslint/no-explicit-any -- Probes inspect third-party prebuilt bundles. */
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'

const installedRoot =
  process.env.TEZBAR_EXTENSION_TEST_ROOT ??
  join(homedir(), 'Library/Application Support/com.tezbar.app/extensions')
const scratch = mkdtempSync(join(tmpdir(), 'tezbar-installed-probes-'))
const names = [
  'audio-device',
  'speedtest',
  'google-search',
  'amphetamine',
  'timers',
  'cleanshotx',
  'perplexity',
  'color-picker',
]
vi.mock('@tezbar/desktop-runtime', () => ({
  app: {
    isPackaged: false,
    getPath: () => scratch,
    getAppPath: () => process.cwd(),
    hide() {},
    show() {},
  },
  BrowserWindow: class {
    static getAllWindows() {
      return []
    }
  },
  clipboard: { readText: () => '', writeText() {}, writeImage() {} },
  nativeImage: { createFromPath: () => ({ isEmpty: () => true }) },
  shell: { openExternal: async () => {}, showItemInFolder() {} },
}))
vi.mock('../../src/main/llm/extensionAI', () => ({ askExtensionAI: async () => 'probe' }))
import {
  clearAllExtensionSessions,
  refreshExtensionSession,
  runExtensionCommandFromPackageJson,
} from '../../src/main/extension-runner'
import { getManifestPlatforms } from '../../src/main/extension-platform'

beforeAll(() => {
  for (const name of names) {
    const source = join(installedRoot, name)
    expect(
      existsSync(join(source, 'package.json')),
      `Missing installed ${name}; set TEZBAR_EXTENSION_TEST_ROOT`
    ).toBe(true)
    const target = join(scratch, name)
    mkdirSync(target)
    // Never copy user preferences, LocalStorage, caches, or native helpers.
    for (const entry of ['package.json', '.sc-build', 'assets']) {
      if (existsSync(join(source, entry)))
        cpSync(join(source, entry), join(target, entry), { recursive: true })
    }
  }
})
afterAll(() => {
  clearAllExtensionSessions()
  rmSync(scratch, { recursive: true, force: true })
})

function preferences(name: string, command: string): Record<string, unknown> {
  const manifest = JSON.parse(readFileSync(join(scratch, name, 'package.json'), 'utf8'))
  return Object.fromEntries(
    [
      ...(manifest.preferences ?? []),
      ...(manifest.commands.find((item: any) => item.name === command)?.preferences ?? []),
    ].map((item: any) => [
      item.name,
      item.default ??
        (item.type === 'checkbox'
          ? false
          : item.type === 'dropdown'
            ? (item.data?.[0]?.value ?? '')
            : ''),
    ])
  )
}

// Only explicitly selected non-destructive views: no native-device changes,
// speedtest transfers, timers started, clipboard access, or account secrets.
describe('installed extension offline smoke views', () => {
  for (const [name, command] of [
    ['audio-device', 'customize-order'],
    ['google-search', 'index'],
    ['timers', 'startCustomTimer'],
    ['timers', 'configureMenubarPresets'],
    ['cleanshotx', 'manage-recording-presets'],
    ['perplexity', 'ask-perplexity'],
    ['color-picker', 'convert-color'],
    ['color-picker', 'organize-colors'],
    ['color-picker', 'color-wheel'],
  ]) {
    it(`${name}/${command}`, async () => {
      const result = await runExtensionCommandFromPackageJson(
        join(scratch, name, 'package.json'),
        command,
        {},
        preferences(name, command),
        { effectMode: 'record' }
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'view') return
      await new Promise((resolve) => setTimeout(resolve, 80))
      const refreshed = await refreshExtensionSession({ sessionId: result.sessionId })
      expect(refreshed.ok, JSON.stringify(refreshed)).toBe(true)
      clearAllExtensionSessions()
    })
  }

  it('CleanShot open-history records the URL without launching the app', async () => {
    const result = await runExtensionCommandFromPackageJson(
      join(scratch, 'cleanshotx/package.json'),
      'open-history',
      {},
      {},
      { effectMode: 'record' }
    )
    expect(result.ok, JSON.stringify(result)).toBe(true)
    if (!result.ok) return
    expect(result.effects).toContainEqual({ kind: 'open', value: 'cleanshot://open-history' })
  })

  it('Perplexity query submits without opening a browser', async () => {
    const result = await runExtensionCommandFromPackageJson(
      join(scratch, 'perplexity/package.json'),
      'ask-perplexity',
      { query: 'test query' },
      { perplexityApp: false },
      { effectMode: 'record' }
    )
    expect(result.ok, JSON.stringify(result)).toBe(true)
    if (!result.ok) return
    expect(result.effects).toContainEqual({
      kind: 'open',
      value: 'https://www.perplexity.ai/search?q=test+query',
    })
  })
})

describe('installed extension declared API surface', () => {
  for (const name of names) {
    it(`${name}: every referenced top-level Raycast export exists`, async () => {
      const refs: Record<string, Set<string>> = { api: new Set(), utils: new Set() }
      const target = join(scratch, name)
      for (const entry of readdirSync(join(target, '.sc-build')).filter((entry) =>
        entry.endsWith('.js')
      )) {
        const code = readFileSync(join(target, '.sc-build', entry), 'utf8')
        for (const module of Object.keys(refs)) {
          for (const alias of code.matchAll(
            new RegExp(`var (\\w+) = (?:__toESM\\()?require\\("@raycast/${module}"\\)`, 'g')
          )) {
            for (const ref of code.matchAll(new RegExp(`\\b${alias[1]}\\.(\\w+)`, 'g')))
              refs[module].add(ref[1])
          }
        }
      }
      const probe = join(scratch, `api-probe-${name}`)
      mkdirSync(join(probe, '.sc-build'), { recursive: true })
      writeFileSync(
        join(probe, 'package.json'),
        JSON.stringify({
          name: `api-probe-${name}`,
          commands: [{ name: 'index', mode: 'no-view' }],
        })
      )
      writeFileSync(
        join(probe, '.sc-build/index.js'),
        `const api = require('@raycast/api'); const utils = require('@raycast/utils'); module.exports.default = async () => { const refs = ${JSON.stringify(Object.fromEntries(Object.entries(refs).map(([k, values]) => [k, [...values]])))}; const missing = Object.entries(refs).flatMap(([module, keys]) => keys.filter(key => ({api,utils})[module][key] === undefined).map(key => module+'.'+key)); await api.Clipboard.copy(JSON.stringify(missing)); };`
      )
      const result = await runExtensionCommandFromPackageJson(
        join(probe, 'package.json'),
        'index',
        {},
        {},
        { effectMode: 'record' }
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'no-view') return
      const missing = result.effects?.find((effect) => effect.kind === 'clipboard')?.value
      expect(missing, `${name} missing API: ${missing}`).toBe('[]')
    })
  }

  it('Amphetamine and CleanShot honestly declare macOS-only support', () => {
    for (const name of ['amphetamine', 'cleanshotx'])
      expect(
        getManifestPlatforms(JSON.parse(readFileSync(join(scratch, name, 'package.json'), 'utf8')))
      ).toEqual(['macOS'])
  })

  it('Timers rejects Windows when its bundled commands rely on POSIX audio/shell', () => {
    const manifest = JSON.parse(readFileSync(join(scratch, 'timers/package.json'), 'utf8'))
    const code = readFileSync(join(scratch, 'timers/.sc-build/start2MinuteTimer.js'), 'utf8')
    expect(code).toContain('afplay')
    expect(getManifestPlatforms(manifest)).toEqual(['macOS'])
  })
})
