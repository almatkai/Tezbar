import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, basename } from 'node:path'
import { afterAll, describe, expect, it, vi } from 'vitest'
import { transformSync } from 'esbuild'
import { LOVED_EXTENSIONS } from '../../src/shared/lovedExtensions'
const root = process.env.TEZBAR_CATALOG_ROOT || '/tmp/tezbar-full-catalog-audit'
vi.mock('@tezbar/desktop-runtime', () => ({
  app: { isPackaged: false, getPath: () => root, getAppPath: () => process.cwd() },
  BrowserWindow: class {
    static getAllWindows() {
      return []
    }
  },
}))
import { buildAllCommands } from '../../src/main/extension-builder'
import {
  isCommandPlatformCompatible,
  isManifestPlatformCompatible,
} from '../../src/main/extension-platform'
const results: Array<{ name: string; platform: string; expected: number; built: number }> = []
// Start the real host-native esbuild service before platform-branch simulation.
transformSync('const fixture = 1')
afterAll(() => {
  writeFileSync(join(root, 'build-results.json'), JSON.stringify(results, null, 2))
  for (const extension of LOVED_EXTENSIONS) {
    const target = join(root, basename(extension.repository || ''))
    const mac = join(target, '.audit-builds/darwin')
    if (existsSync(mac)) cpSync(mac, join(target, '.sc-build'), { recursive: true })
  }
})
describe('all curated extensions build with the production builder', () => {
  for (const platform of ['darwin', 'win32']) {
    for (const extension of LOVED_EXTENSIONS) {
      const name = basename(extension.repository || '')
      it(`${platform}: ${name}`, async () => {
        const original = Object.getOwnPropertyDescriptor(process, 'platform')!
        Object.defineProperty(process, 'platform', { value: platform })
        try {
          const target = join(root, name)
          const pkg = JSON.parse(readFileSync(join(target, 'package.json'), 'utf8'))
          // Never let the builder auto-install or run source-package lifecycle scripts.
          if (!existsSync(join(target, 'node_modules/@aternus/csv-to-xlsx/package.json')))
            mkdirSync(join(target, 'node_modules'), { recursive: true })
          const expected = isManifestPlatformCompatible(pkg)
            ? pkg.commands.filter(isCommandPlatformCompatible).length
            : 0
          const built = await buildAllCommands(name, target)
          results.push({ name, platform, expected, built })
          const bundles = join(target, '.sc-build')
          if (existsSync(bundles))
            cpSync(bundles, join(target, '.audit-builds', platform), { recursive: true })
          expect(built, `${name}: ${built}/${expected} bundles`).toBe(expected)
        } finally {
          Object.defineProperty(process, 'platform', original)
        }
      }, 30_000)
    }
  }
})
