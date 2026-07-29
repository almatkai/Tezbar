import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'

import { buildAllCommands } from './extension-builder'

const temporaryDirectories: string[] = []

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true })
  }
})

describe('extension command builds', () => {
  it('keeps Raycast rust modules external for the runtime shim', async () => {
    const extensionRoot = mkdtempSync(join(tmpdir(), 'tezbar-native-extension-'))
    temporaryDirectories.push(extensionRoot)
    mkdirSync(join(extensionRoot, 'src'), { recursive: true })
    writeFileSync(
      join(extensionRoot, 'package.json'),
      JSON.stringify({
        name: 'native-extension-fixture',
        commands: [{ name: 'native-command', mode: 'view' }],
      }),
      'utf8'
    )
    writeFileSync(
      join(extensionRoot, 'src', 'native-command.ts'),
      [
        'export default async function command(): Promise<unknown> {',
        '  return import("rust:../rust/native-command")',
        '}',
      ].join('\n'),
      'utf8'
    )

    await expect(buildAllCommands('native-extension-fixture', extensionRoot)).resolves.toBe(1)
    expect(existsSync(join(extensionRoot, '.sc-build', 'native-command.js'))).toBe(true)
  })
})
