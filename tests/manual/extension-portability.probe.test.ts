/* eslint-disable @typescript-eslint/no-explicit-any -- Ad-hoc VM probes load untyped third-party extension exports. */
import {
  cpSync,
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import { execFileSync } from 'node:child_process'
import { createServer } from 'node:net'
import { build } from 'esbuild'
import { afterAll, describe, expect, it, vi } from 'vitest'

const scratch = mkdtempSync(join(tmpdir(), 'tezbar-portability-'))
const sourceRoot = resolve('../Tezbar Extensions/extensions')
const realPlatform = process.platform
const descriptor = Object.getOwnPropertyDescriptor(process, 'platform')!
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
import { buildAllCommands } from '../../src/main/extension-builder'
import {
  clearAllExtensionSessions,
  refreshExtensionSession,
  runExtensionCommandFromPackageJson,
  updateSearchText,
} from '../../src/main/extension-runner'

afterAll(() => {
  Object.defineProperty(process, 'platform', descriptor)
  clearAllExtensionSessions()
  rmSync(scratch, { recursive: true, force: true })
})

async function bundleModule(
  relative: string,
  platform: string,
  mocks: Record<string, unknown> = {}
) {
  const filename = join(sourceRoot, relative)
  const result = await build({
    entryPoints: [filename],
    bundle: true,
    write: false,
    format: 'cjs',
    platform: 'node',
    packages: 'external',
    define: { 'process.platform': JSON.stringify(platform) },
    logLevel: 'silent',
  })
  const module = { exports: {} as any }
  const nativeRequire = createRequire(filename)
  runInNewContext(result.outputFiles![0].text, {
    module,
    exports: module.exports,
    process,
    Buffer,
    console,
    setTimeout,
    clearTimeout,
    require: (name: string) => (Object.hasOwn(mocks, name) ? mocks[name] : nativeRequire(name)),
  })
  return module.exports
}

const names = [
  'gif-search',
  'google-calendar',
  'google-translate',
  'kill-process',
  'port-manager',
  'spotify-player',
  'visual-studio-code-recent-projects',
]
describe('actual extension manifests and Tezbar builds (Windows branches simulated, not a Windows host)', () => {
  for (const platform of ['darwin', 'win32'])
    for (const name of names) {
      it(`${platform}: builds every compatible command in ${name}`, async () => {
        const source = join(sourceRoot, name)
        const target = join(scratch, `${platform}-${name}`)
        mkdirSync(target)
        for (const entry of ['src', 'vendor', 'assets', 'package.json', 'tsconfig.json']) {
          if (existsSync(join(source, entry)))
            cpSync(join(source, entry), join(target, entry), { recursive: true })
        }
        symlinkSync(join(source, 'node_modules'), join(target, 'node_modules'), 'dir')
        const manifest = JSON.parse(readFileSync(join(target, 'package.json'), 'utf8'))
        const expected = manifest.commands.filter(
          (command: any) =>
            !command.platforms ||
            command.platforms.includes(platform === 'win32' ? 'Windows' : 'macOS')
        ).length
        Object.defineProperty(process, 'platform', { ...descriptor, value: platform })
        try {
          expect(await buildAllCommands(name, target)).toBe(expected)
        } finally {
          Object.defineProperty(process, 'platform', descriptor)
        }
      }, 30000)
    }
})

describe('real macOS local-view smoke tests (no actions invoked)', () => {
  for (const [name, command] of [
    ['gif-search', 'favorites'],
    ['gif-search', 'recents'],
    ['google-translate', 'translate'],
    ['google-translate', 'translate-form'],
    ['kill-process', 'index'],
    ['port-manager', 'named-ports'],
    ['port-manager', 'open-ports'],
  ]) {
    it(`${name}/${command} opens in the real Tezbar runtime`, async () => {
      const target = join(scratch, `darwin-${name}`)
      const manifest = JSON.parse(readFileSync(join(target, 'package.json'), 'utf8'))
      const preferences = Object.fromEntries(
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
      preferences.autoInput = false
      const result = await runExtensionCommandFromPackageJson(
        join(target, 'package.json'),
        command,
        undefined,
        preferences,
        { effectMode: 'record' }
      )
      expect(result.ok, JSON.stringify(result)).toBe(true)
      if (!result.ok || result.mode !== 'view') return
      await new Promise((resolve) => setTimeout(resolve, 150))
      const updated = await refreshExtensionSession({ sessionId: result.sessionId })
      expect(updated.ok, JSON.stringify(updated)).toBe(true)
      console.log('SMOKE', name, command, JSON.stringify(updated).slice(0, 650))
      clearAllExtensionSessions()
    })
  }
})

describe('cross-platform API contracts', () => {
  it('Port Manager macOS detects a controlled real TCP listening socket', async () => {
    const server = createServer()
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
    try {
      const address = server.address() as { port: number }
      const Process = (
        await bundleModule('port-manager/src/models/Process.ts', 'darwin', {
          '@raycast/api': {
            Cache: class {
              get() {
                return undefined
              }
            },
          },
          '@raycast/utils': {},
        })
      ).default
      const processes = await Process.getCurrent()
      expect(
        processes.some((item: any) =>
          item.portInfo?.some((port: any) => port.port === address.port)
        ),
        JSON.stringify({ port: address.port, processes })
      ).toBe(true)
      const target = join(scratch, 'darwin-port-manager')
      const initial = await runExtensionCommandFromPackageJson(
        join(target, 'package.json'),
        'open-ports',
        undefined,
        {},
        { effectMode: 'record' }
      )
      expect(initial.ok, JSON.stringify(initial)).toBe(true)
      if (!initial.ok || initial.mode !== 'view') return
      // Other local applications may occupy the first 30 rows. Active search
      // exposes all client-filtered items instead of testing just page one.
      let current = await updateSearchText({
        sessionId: initial.sessionId,
        searchText: String(address.port),
      })
      await vi.waitFor(
        async () => {
          const refreshed = await refreshExtensionSession({ sessionId: initial.sessionId })
          if (refreshed.ok && refreshed.mode === 'view') current = refreshed
          expect(
            JSON.stringify(current),
            `Runtime did not expose controlled port ${address.port}`
          ).toContain(String(address.port))
        },
        { timeout: 3000, interval: 50 }
      )
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()))
    }
  })

  it('Tezbar spawn preserves stdout line delimiters required by lsof/netstat parsers on both OSs', async () => {
    const target = join(scratch, 'spawn-api')
    mkdirSync(join(target, '.sc-build'), { recursive: true })
    writeFileSync(
      join(target, 'package.json'),
      JSON.stringify({ name: 'spawn-api', commands: [{ name: 'index', mode: 'no-view' }] })
    )
    writeFileSync(
      join(target, '.sc-build/index.js'),
      `const {spawn} = require('child_process'); const {Clipboard} = require('@raycast/api'); module.exports.default = async () => { const stdout = await new Promise((resolve, reject) => { const child = spawn(process.execPath, ['-e', 'process.stdout.write("first\\\\nsecond\\\\n")']); let output = ''; child.stdout.on('data', chunk => output += chunk); child.on('error', reject); child.on('close', () => resolve(output)); }); await Clipboard.copy(stdout); };`
    )
    const result = await runExtensionCommandFromPackageJson(
      join(target, 'package.json'),
      'index',
      undefined,
      {},
      { effectMode: 'record' }
    )
    expect(result.ok, JSON.stringify(result)).toBe(true)
    if (!result.ok || result.mode !== 'no-view') return
    expect(result.effects).toContainEqual({ kind: 'clipboard', value: 'first\nsecond\n' })
  })

  it('Tezbar runtime supplies runPowerShellScript used by Spotify and VSCode on Windows', async () => {
    const target = join(scratch, 'powershell-api')
    mkdirSync(join(target, '.sc-build'), { recursive: true })
    writeFileSync(
      join(target, 'package.json'),
      JSON.stringify({ name: 'powershell-api', commands: [{ name: 'index', mode: 'no-view' }] })
    )
    writeFileSync(
      join(target, '.sc-build/index.js'),
      `const {runPowerShellScript} = require('@raycast/utils'); module.exports.default = async () => { if(typeof runPowerShellScript !== 'function') throw new Error('runPowerShellScript is missing from Tezbar runtime'); };`
    )
    const result = await runExtensionCommandFromPackageJson(
      join(target, 'package.json'),
      'index',
      undefined,
      {},
      { effectMode: 'record' }
    )
    expect(result.ok, JSON.stringify(result)).toBe(true)
  })

  it('VSCode on macOS sends an executable path without literal quote characters', async () => {
    const calls: any[][] = []
    const vscode = await bundleModule(
      'visual-studio-code-recent-projects/src/lib/vscode.ts',
      'darwin',
      {
        '@raycast/api': { getPreferenceValues: () => ({ build: 'Code' }), open() {} },
        '@raycast/utils': {},
        child_process: { execFileSync: (...args: any[]) => calls.push(args) },
      }
    )
    vscode.getVSCodeCLI().openFolderURISync('file:///tmp/probe')
    expect(calls).toHaveLength(1)
    expect(calls[0][0].startsWith('"'), JSON.stringify(calls[0])).toBe(false)
  })

  it.runIf(realPlatform === 'darwin')(
    'VSCode macOS invokes an existing CLI fixture without ENOENT',
    async () => {
      const filename = join(
        scratch,
        'Applications/Visual Studio Code.app/Contents/Resources/app/bin/code'
      )
      mkdirSync(join(filename, '..'), { recursive: true })
      writeFileSync(filename, '#!/bin/sh\nexit 0\n')
      chmodSync(filename, 0o755)
      const nativeRequire = createRequire(import.meta.url)
      const vscode = await bundleModule(
        'visual-studio-code-recent-projects/src/lib/vscode.ts',
        'darwin',
        {
          '@raycast/api': { getPreferenceValues: () => ({ build: 'Code' }), open() {} },
          '@raycast/utils': {},
          os: { ...nativeRequire('node:os'), homedir: () => scratch },
          fs: {
            ...nativeRequire('node:fs'),
            existsSync: (candidate: string) => candidate === filename,
          },
          child_process: {
            execFileSync: (executable: string, args: string[], options: object) => {
              expect(executable).toBe(filename)
              return execFileSync(executable, args, { ...options, timeout: 2000 })
            },
          },
        }
      )
      expect(() => vscode.getVSCodeCLI().openFolderURISync('file:///tmp/probe')).not.toThrow()
    }
  )

  it('Google Translate TTS on Windows does not invoke macOS afplay', async () => {
    const calls: any[][] = []
    const translator = await bundleModule('google-translate/src/simple-translate.ts', 'win32', {
      '@iamtraction/translate': {},
      'google-tts-api': { getAudioUrl: () => 'https://example.invalid/probe.mp3' },
      https: {
        get: (_url: string, _options: unknown, callback: any) => {
          const response = {
            statusCode: 200,
            on: (event: string, handler: any) => {
              if (event === 'data') handler(Buffer.from('fixture'))
              return response
            },
            once: (event: string, handler: any) => {
              if (event === 'end') handler()
              return response
            },
          }
          callback(response)
          const request = { once: () => request, setTimeout: () => request }
          return request
        },
      },
      fs: { mkdtempSync: () => '/tmp/tts-fixture', writeFileSync() {}, rmSync() {} },
      child_process: {
        spawn: (...args: any[]) => {
          calls.push(args)
          return {
            once(event: string, handler: any) {
              if (event === 'close') handler(0)
              return this
            },
          }
        },
      },
    })
    await translator.playTTS('hello', 'en')
    expect(calls).toHaveLength(1)
    expect(calls[0][0]).not.toBe('afplay')
  })

  it('Spotify Windows play selects PowerShell without AppleScript; volume explicitly rejects non-premium', async () => {
    const scripts: string[] = []
    const spotify = await bundleModule('spotify-player/src/helpers/script.ts', 'win32', {
      '@raycast/api': { getPreferenceValues: () => ({}) },
      '@raycast/utils': {
        runPowerShellScript: async (script: string) => scripts.push(script),
        runAppleScript: () => {
          throw Error('AppleScript called on Windows')
        },
      },
    })
    await spotify.runSpotifyScript(spotify.SpotifyScriptType.Play)
    expect(scripts[0]).toContain('TryPlayAsync')
    await expect(
      spotify.runSpotifyScript(spotify.SpotifyScriptType.SetVolume, false, 50)
    ).rejects.toThrow('not supported on Windows')
  })

  it('Kill Process generates Windows listing/restart/tree commands safely without running them', async () => {
    const platform = await bundleModule('kill-process/src/utils/platform.ts', 'win32', {
      '@raycast/api': { Image: { Mask: {} } },
    })
    const spec = platform.getProcessListCommandSpec()
    expect(spec.executable).toBe('powershell')
    expect(Buffer.from(spec.args.at(-1), 'base64').toString('utf16le')).toContain('Get-Process')
    expect(platform.getKillTreeCommand(12345, true)).toBe('taskkill /F /T /PID 12345')
    const command = platform.getRestartCommand({ path: "C:\\Users\\O'Brien\\App.exe", type: 'app' })
    expect(Buffer.from(command.split(' ').at(-1), 'base64').toString('utf16le')).toContain(
      "O''Brien"
    )
  })

  it.runIf(realPlatform === 'darwin')(
    'Kill Process lists processes with real macOS ps (read-only)',
    async () => {
      const platform = await bundleModule('kill-process/src/utils/platform.ts', 'darwin', {
        '@raycast/api': { Image: { Mask: {} } },
      })
      const spec = platform.getProcessListCommandSpec()
      const output = execFileSync(spec.executable, spec.args, { encoding: 'utf8' })
      const parsed = output.split('\n').map(platform.parseProcessLine).filter(Boolean)
      expect(parsed.some((item: any) => item.id === process.pid)).toBe(true)
    }
  )
})
