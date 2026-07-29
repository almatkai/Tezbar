import * as path from 'path'
import { describe, expect, it } from 'vitest'
import { createBunInstallEnvironment } from './bun-manager'

describe('createBunInstallEnvironment', () => {
  it('does not leak Tezbar esbuild overrides into extension installers', () => {
    const source = {
      ESBUILD_BINARY_PATH: 'C:\\tezbar\\esbuild.exe',
      PATH: 'C:\\Windows\\System32',
      KEEP_ME: 'yes',
    }

    const env = createBunInstallEnvironment('C:\\Tezbar\\bun\\bun.exe', source)

    expect(env.ESBUILD_BINARY_PATH).toBeUndefined()
    expect(env.KEEP_ME).toBe('yes')
    expect(env.PATH).toBe([`C:\\Tezbar\\bun`, source.PATH].join(path.delimiter))
    expect(source.ESBUILD_BINARY_PATH).toBe('C:\\tezbar\\esbuild.exe')
  })

  it('removes case variants of the esbuild override', () => {
    const env = createBunInstallEnvironment('/opt/tezbar/bun', {
      Esbuild_Binary_Path: '/opt/tezbar/esbuild',
      PATH: '/usr/bin',
    })

    expect(Object.keys(env).some((key) => key.toUpperCase() === 'ESBUILD_BINARY_PATH')).toBe(false)
  })
})
