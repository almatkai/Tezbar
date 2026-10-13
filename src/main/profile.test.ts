import { describe, expect, it } from 'vitest'
import { join } from 'node:path'
import { configurationDirectory, defaultLauncherShortcut, piAgentDirectory } from './profile'

describe('release profile compatibility', () => {
  it.each(['stable', 'beta', 'alpha'])(
    'keeps %s on the existing settings and agent profile',
    (channel) => {
      const env = { TEZBAR_RELEASE_CHANNEL: channel }
      expect(configurationDirectory(env, '/home/test')).toBe(join('/home/test', '.openray'))
      expect(piAgentDirectory(env, '/home/test')).toBe(join('/home/test', '.pi', 'agent'))
    }
  )
  it('supports temporary profiles without changing production defaults', () => {
    const env = { TEZBAR_CONFIG_DIR: '/test/config', PI_CODING_AGENT_DIR: '/test/pi' }
    expect(configurationDirectory(env, '/home/test')).toBe('/test/config')
    expect(piAgentDirectory(env, '/home/test')).toBe('/test/pi')
    expect(defaultLauncherShortcut('win32')).toBe('Control+Space')
    expect(defaultLauncherShortcut('darwin')).toBe('Alt+Space')
  })
})
