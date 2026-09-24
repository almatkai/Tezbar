import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'

const temporaryHomes: string[] = []

async function isolatedConfigStore() {
  const home = mkdtempSync(join(tmpdir(), 'tezbar-config-store-'))
  temporaryHomes.push(home)
  vi.stubEnv('HOME', home)
  vi.resetModules()
  return import('./configStore')
}

afterEach(() => {
  vi.unstubAllEnvs()
  for (const home of temporaryHomes.splice(0)) {
    rmSync(home, { recursive: true, force: true })
  }
})

describe.sequential('configStore persistence', () => {
  it('writes an LLM config patch to disk immediately', async () => {
    const store = await isolatedConfigStore()

    store.writeConfigPatch({
      customProviders: [{ id: 'custom:model', title: 'Model' }],
      provider: 'custom:model',
      baseURL: 'http://127.0.0.1:8080/v1',
    })

    expect(JSON.parse(readFileSync(store.OPENRAY_CONFIG_PATH, 'utf8'))).toMatchObject({
      customProviders: [{ id: 'custom:model', title: 'Model' }],
      provider: 'custom:model',
      baseURL: 'http://127.0.0.1:8080/v1',
    })
  })

  it('does not overwrite a newer disk config during a clean shutdown flush', async () => {
    const store = await isolatedConfigStore()
    store.writeConfigPatch({ provider: 'ollama' })

    writeFileSync(
      store.OPENRAY_CONFIG_PATH,
      `${JSON.stringify({ provider: 'custom:model' }, null, 2)}\n`,
      'utf8'
    )
    store.flushConfig()

    expect(JSON.parse(readFileSync(store.OPENRAY_CONFIG_PATH, 'utf8'))).toEqual({
      provider: 'custom:model',
    })
  })
})
