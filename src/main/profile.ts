import { homedir } from 'node:os'
import { join } from 'node:path'

export function configurationDirectory(env = process.env, home = homedir()): string {
  if (env.TEZBAR_CONFIG_DIR) return env.TEZBAR_CONFIG_DIR
  return join(home, '.openray')
}

export function piAgentDirectory(env = process.env, home = homedir()): string {
  if (env.PI_CODING_AGENT_DIR) return env.PI_CODING_AGENT_DIR
  return join(home, '.pi', 'agent')
}

export function defaultLauncherShortcut(platform = process.platform): string {
  const modifier = platform === 'win32' ? 'Control' : 'Alt'
  return `${modifier}+Space`
}
