import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, describe, expect, it, vi } from 'vitest'
import { LOVED_EXTENSIONS } from '../../src/shared/lovedExtensions'
const scratch = mkdtempSync(join(tmpdir(), 'tezbar-catalog-api-'))
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
  runExtensionCommandFromPackageJson,
} from '../../src/main/extension-runner'
type Entry = { name: string; id: string; refs: Record<string, string[]>; error?: string }
const inventory: Entry[] = JSON.parse(
  readFileSync(
    process.env.TEZBAR_CATALOG_AUDIT_JSON || '/tmp/tezbar-catalog-source-audit.json',
    'utf8'
  )
)
afterAll(() => {
  clearAllExtensionSessions()
  rmSync(scratch, { recursive: true, force: true })
})

describe('complete curated catalog API footprint (does not execute extensions)', () => {
  it('covers every current catalog entry without silently dropping unavailable sources', () => {
    expect(inventory.filter((entry) => entry.error)).toEqual([])
    expect(inventory.map((entry) => entry.id).sort()).toEqual(
      LOVED_EXTENSIONS.map((entry) => entry.id).sort()
    )
  })
  for (const entry of inventory) {
    it(`${entry.name}: every referenced runtime export exists`, async () => {
      const target = join(scratch, entry.name)
      mkdirSync(join(target, '.sc-build'), { recursive: true })
      writeFileSync(
        join(target, 'package.json'),
        JSON.stringify({ name: entry.name, commands: [{ name: 'index', mode: 'no-view' }] })
      )
      writeFileSync(
        join(target, '.sc-build/index.js'),
        `const modules = { api: require('@raycast/api'), utils: require('@raycast/utils') }; module.exports.default = async () => { const refs = ${JSON.stringify(entry.refs)}; const missing = Object.entries(refs).flatMap(([module, keys]) => keys.filter(key => modules[module][key] === undefined).map(key => module+'.'+key)); await modules.api.Clipboard.copy(JSON.stringify(missing)); };`
      )
      const result = await runExtensionCommandFromPackageJson(
        join(target, 'package.json'),
        'index',
        {},
        {},
        { effectMode: 'record' }
      )
      expect(result.ok).toBe(true)
      if (!result.ok) return
      const missing = result.effects?.find((effect) => effect.kind === 'clipboard')?.value
      expect(missing, `${entry.name}: missing ${missing}`).toBe('[]')
    })
  }
})
