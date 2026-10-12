import { describe, expect, it } from 'vitest'
import { join } from 'node:path'
import { configurationDirectory, defaultLauncherShortcut, piAgentDirectory } from './profile'

describe('release profile isolation', () => {
  it('keeps normal installations compatible with existing settings', () => {
    expect(configurationDirectory({}, '/home/test')).toBe(join('/home/test', '.openray'))
    expect(configurationDirectory({ TEZBAR_RELEASE_CHANNEL: 'beta' }, '/home/test')).toBe(
      join('/home/test', '.openray')
    )
  })
  it('isolates Alpha settings, agent sessions, and default shortcuts', () => {
    const env = { TEZBAR_RELEASE_CHANNEL: 'alpha', APPDATA_DIR: '/profiles/alpha' }
    expect(configurationDirectory(env, '/home/test')).toBe(join('/profiles/alpha', 'config'))
    expect(piAgentDirectory(env, '/home/test')).toBe(join('/profiles/alpha', 'pi-agent'))
    expect(defaultLauncherShortcut('win32', 'alpha')).toBe('Control+Shift+Space')
    expect(defaultLauncherShortcut('darwin', 'alpha')).toBe('Alt+Shift+Space')
    expect(defaultLauncherShortcut('win32', 'stable')).toBe('Control+Space')
  })
})
