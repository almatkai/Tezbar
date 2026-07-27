import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join } from 'node:path'
import { DEFAULT_EXTENSION_RUNTIME_TIMEOUT_MS } from '../../shared/llmConfig'

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

/** How long an extension view may stay idle before returning to CommandBar. */
export type CommandSurfaceTimeoutKey =
  | 'extensionRuntimeTimeoutMs'
  | 'aiModeTimeoutMs'
  | 'terminalModeTimeoutMs'

export function getCommandSurfaceTimeoutMs(key: CommandSurfaceTimeoutKey): number {
  const raw = readRawConfig()[key]
  if (typeof raw === 'number' && Number.isFinite(raw) && raw >= 0) {
    return raw
  }
  return DEFAULT_EXTENSION_RUNTIME_TIMEOUT_MS
}

export function getExtensionRuntimeTimeoutMs(): number {
  return getCommandSurfaceTimeoutMs('extensionRuntimeTimeoutMs')
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

function normalizeAgentAlwaysAllowedExactCommand(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const command = value.trim()
  return command && command.length <= 16_384 && !command.includes('\0') ? command : null
}

export function getAgentAlwaysAllowedExactCommands(): string[] {
  const value = readRawConfig().agentAlwaysAllowedExactCommands
  if (!Array.isArray(value)) return []
  return Array.from(
    new Set(
      value
        .map(normalizeAgentAlwaysAllowedExactCommand)
        .filter((command): command is string => command !== null)
    )
  )
}

export function addAgentAlwaysAllowedExactCommand(command: string): void {
  const normalized = normalizeAgentAlwaysAllowedExactCommand(command)
  if (!normalized) return
  writeConfigPatch({
    agentAlwaysAllowedExactCommands: Array.from(
      new Set([...getAgentAlwaysAllowedExactCommands(), normalized])
    ),
  })
}

export type PersistedWindowPosition = { x: number; y: number }

function isPersistedWindowPosition(value: unknown): value is PersistedWindowPosition {
  return (
    !!value &&
    typeof value === 'object' &&
    typeof (value as PersistedWindowPosition).x === 'number' &&
    typeof (value as PersistedWindowPosition).y === 'number' &&
    Number.isFinite((value as PersistedWindowPosition).x) &&
    Number.isFinite((value as PersistedWindowPosition).y)
  )
}

export function getPersistedWindowPosition(): PersistedWindowPosition | null {
  const raw = readRawConfig()
  const pos = raw.windowPosition
  if (isPersistedWindowPosition(pos)) {
    return pos
  }
  return null
}

export function setPersistedWindowPosition(pos: PersistedWindowPosition): void {
  writeConfigPatch({ windowPosition: pos })
}

export function getPersistedWindowPositionForDisplay(
  displayKey: string
): PersistedWindowPosition | null {
  const raw = readRawConfig()
  const positions = raw.windowPositionsByDisplay
  if (!positions || typeof positions !== 'object' || Array.isArray(positions)) return null
  const pos = (positions as Record<string, unknown>)[displayKey]
  return isPersistedWindowPosition(pos) ? pos : null
}

export function setPersistedWindowPositionForDisplay(
  displayKey: string,
  pos: PersistedWindowPosition
): void {
  const raw = readRawConfig()
  const current =
    raw.windowPositionsByDisplay &&
    typeof raw.windowPositionsByDisplay === 'object' &&
    !Array.isArray(raw.windowPositionsByDisplay)
      ? (raw.windowPositionsByDisplay as Record<string, PersistedWindowPosition>)
      : {}
  writeConfigPatch({
    windowPosition: pos,
    windowPositionsByDisplay: {
      ...current,
      [displayKey]: pos,
    },
  })
}

export function getRaymesHotkey(): string {
  const value = readRawConfig().raymesHotkey
  return typeof value === 'string' && value.trim() ? value : DEFAULT_RAYMES_HOTKEY
}

export function setRaymesHotkey(accelerator: string): void {
  writeConfigPatch({ raymesHotkey: accelerator })
}

export function getCommandHotkeys(): Record<string, string> {
  const value = readRawConfig().commandHotkeys
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, string>
  }
  return {}
}

export function setCommandHotkeys(hotkeys: Record<string, string>): void {
  writeConfigPatch({ commandHotkeys: hotkeys })
}

export function getCommandAliases(): Record<string, string> {
  const value = readRawConfig().commandAliases
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, string>
  }
  return {}
}

export function setCommandAliases(aliases: Record<string, string>): void {
  writeConfigPatch({ commandAliases: aliases })
}

export function getDisabledCommands(): Record<string, boolean> {
  const value = readRawConfig().disabledCommands
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, boolean>
  }
  return {}
}

export function setDisabledCommands(disabled: Record<string, boolean>): void {
  writeConfigPatch({ disabledCommands: disabled })
}
