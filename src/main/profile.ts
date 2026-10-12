import { homedir } from 'node:os'
import { join } from 'node:path'

export function configurationDirectory(env = process.env, home = homedir()): string {
  if (env.TEZBAR_CONFIG_DIR) return env.TEZBAR_CONFIG_DIR
  if (env.TEZBAR_RELEASE_CHANNEL === 'alpha')
    return join(env.APPDATA_DIR || join(home, '.tezbar-alpha'), 'config')
  return join(home, '.openray')
}

export function piAgentDirectory(env = process.env, home = homedir()): string {
  if (env.PI_CODING_AGENT_DIR) return env.PI_CODING_AGENT_DIR
  if (env.TEZBAR_RELEASE_CHANNEL === 'alpha')
    return join(env.APPDATA_DIR || join(home, '.tezbar-alpha'), 'pi-agent')
  return join(home, '.pi', 'agent')
}

export function defaultLauncherShortcut(
  platform = process.platform,
  channel = process.env.TEZBAR_RELEASE_CHANNEL
): string {
  const modifier = platform === 'win32' ? 'Control' : 'Alt'
  return channel === 'alpha' ? `${modifier}+Shift+Space` : `${modifier}+Space`
}
