import { mkdtempSync, rmSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { resolveLauncherDirectory, resolveSlashPathInput } from './service'

const createdDirectories: string[] = []

afterEach(() => {
  for (const directory of createdDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true })
  }
})

describe('launcher directory resolution', () => {
  it('expands compact slash paths below the user home directory', () => {
    expect(resolveSlashPathInput('/Desktop/code/aml')).toBe(join(homedir(), 'Desktop/code/aml'))
  })

  it('accepts existing directories and rejects missing paths', () => {
    const directory = mkdtempSync(join(tmpdir(), 'tezbar-agent-cwd-'))
    createdDirectories.push(directory)

    expect(resolveLauncherDirectory(directory)).toBe(directory)
    expect(resolveLauncherDirectory(join(directory, 'missing'))).toBeNull()
  })
})
