import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

// Workers inherit an isolated profile even when a test forgets its own fixture.
// Teardown runs after workers exit, releasing Windows SQLite file handles.
export default function setup(): () => void {
  const directory = mkdtempSync(join(tmpdir(), 'tezbar-test-profile-'))
  const overrides = {
    APPDATA_DIR: join(directory, 'app-data'),
    TEZBAR_CONFIG_DIR: join(directory, 'config'),
    PI_CODING_AGENT_DIR: join(directory, 'pi-agent'),
  }
  const previous = new Map<string, string | undefined>()
  for (const [key, value] of Object.entries(overrides)) {
    previous.set(key, process.env[key])
    mkdirSync(value, { recursive: true })
    process.env[key] = value
  }
  return () => {
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key]
      else process.env[key] = value
    }
    rmSync(directory, { recursive: true, force: true })
  }
}
