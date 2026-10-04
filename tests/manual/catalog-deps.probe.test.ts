import { execFile } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'
import { afterAll, describe, expect, it } from 'vitest'
const execFileAsync = promisify(execFile)
const roots: string[] = []
afterAll(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true })
})

/** Bun supports remote tarball dependencies; verify the production install
 *  path accepts them without executing package lifecycle scripts. */ describe('extension dependency installation with Bun', () => {
  it('installs the csv-to-excel manifest including its remote tarball', async () => {
    const root = mkdtempSync(join(tmpdir(), 'tezbar-bun-csv-'))
    roots.push(root)
    const manifest = JSON.parse(
      readFileSync('/tmp/tezbar-full-catalog-audit/csv-to-excel/package.json', 'utf8')
    )
    writeFileSync(
      join(root, 'package.json'),
      JSON.stringify({
        name: 'audit-csv-to-excel',
        private: true,
        dependencies: manifest.dependencies,
      })
    )
    await execFileAsync('bun', ['install', '--production', '--no-save'], {
      cwd: root,
      timeout: 120_000,
      env: { ...process.env, BUN_INSTALL_CACHE_DIR: join(root, '.bun-cache') },
    })
    const xlsx = JSON.parse(readFileSync(join(root, 'node_modules/xlsx/package.json'), 'utf8'))
    expect(xlsx.version).toBe('0.20.1')
    expect(readFileSync(join(root, 'node_modules/xlsx/xlsx.js'), 'utf8')).toContain('XLSX')
  }, 120_000)
})
