import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join } from 'node:path'

export const OPENRAY_CONFIG_DIR = join(homedir(), '.openray')
export const OPENRAY_CONFIG_PATH = join(OPENRAY_CONFIG_DIR, 'config.json')
export const DEFAULT_RAYMES_HOTKEY = 'Alt+Space'

let configCache: Record<string, unknown> | null = null

export function readRawConfig(): Record<string, unknown> {
  if (configCache) return configCache

  if (!existsSync(OPENRAY_CONFIG_PATH)) {
    configCache = {}
    return configCache
  }
  try {
    const raw = readFileSync(OPENRAY_CONFIG_PATH, 'utf-8')
    configCache = JSON.parse(raw) as Record<string, unknown>
    return configCache
  } catch {
    configCache = {}
    return configCache
  }
}

let writeTimeout: ReturnType<typeof setTimeout> | null = null

export function flushConfig(): void {
  if (!configCache || !writeTimeout) return
  try {
    mkdirSync(dirname(OPENRAY_CONFIG_PATH), { recursive: true })
    writeFileSync(OPENRAY_CONFIG_PATH, `${JSON.stringify(configCache, null, 2)}\n`, 'utf-8')
    if (writeTimeout) {
      clearTimeout(writeTimeout)
      writeTimeout = null
    }
  } catch (err) {
    console.error('Failed to write config:', err)
  }
}

export function writeConfigPatch(patch: Record<string, unknown>): void {
  const current = readRawConfig()
  configCache = { ...current, ...patch }

  if (writeTimeout) clearTimeout(writeTimeout)
  writeTimeout = setTimeout(() => {
    flushConfig()
    writeTimeout = null
  }, 1000) // Batch writes every 1s
}

/** How long (ms) after hiding the palette we keep UI state (e.g. Providers) when reopening. Default 60s. */
export function getUiStateRetentionMs(): number {
  const raw = readRawConfig()
  const v = raw.uiStateRetentionMs
  if (typeof v === 'number' && Number.isFinite(v) && v >= 0) {
    return v
  }
  return 60_000
}

export function getSafetyDryRun(): boolean {
  const raw = readRawConfig()
  return raw.safetyDryRun === true
}

export function setSafetyDryRun(value: boolean): void {
  writeConfigPatch({ safetyDryRun: value })
}

export function getAgentAlwaysAllowedCommands(): string[] {
  const value = readRawConfig().agentAlwaysAllowedCommands
  if (!Array.isArray(value)) return []
  return Array.from(
    new Set(
      value.filter(
        (entry): entry is string =>
          typeof entry === 'string' && /^[a-z0-9][a-z0-9._+-]{0,63}$/i.test(entry)
      )
    )
  )
}

export function addAgentAlwaysAllowedCommand(command: string): void {
  if (!/^[a-z0-9][a-z0-9._+-]{0,63}$/i.test(command)) return
  writeConfigPatch({
    agentAlwaysAllowedCommands: Array.from(
      new Set([...getAgentAlwaysAllowedCommands(), command.toLowerCase()])
    ),
  })
}

export function getPersistedWindowPosition(): { x: number; y: number } | null {
  const raw = readRawConfig()
  const pos = raw.windowPosition as { x: number; y: number } | undefined
  if (pos && typeof pos.x === 'number' && typeof pos.y === 'number') {
    return pos
  }
  return null
}

export function setPersistedWindowPosition(pos: { x: number; y: number }): void {
  writeConfigPatch({ windowPosition: pos })
}

export function getRaymesHotkey(): string {
  const value = readRawConfig().raymesHotkey
  return typeof value === 'string' && value.trim() ? value : DEFAULT_RAYMES_HOTKEY
}

export function setRaymesHotkey(accelerator: string): void {
  writeConfigPatch({ raymesHotkey: accelerator })
}
