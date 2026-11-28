export type ExtensionPlatform = 'macOS' | 'Windows'
export type ExtensionRuntimeKind = 'rust' | 'mixed' | 'javascript'

export type NativeV2Command = {
  name: string
  title?: string
  runtime: 'rust' | 'javascript' | 'unavailable'
  platforms: ExtensionPlatform[]
  gap?: string
  timeoutMs?: number
  concurrency?: 'serialize' | 'reject-while-busy'
  launcher?: import('./search').LauncherCommandMetadata
  refreshMs?: number | null
}

export type NativeV2Verification = {
  platform: ExtensionPlatform
  state: 'passed' | 'failed' | 'not-run'
  checkedAt?: number
  method?: 'automated' | 'manual'
  note?: string
}

export type NativeV2Info = {
  schemaVersion: 2 | 3
  execution?: 'process'
  protocolVersion?: 1
  repository?: string
  artifacts?: Record<
    string,
    { url: string; sha256: string; executable: string; executableSha256: string }
  >
  launcherContributions?: Array<{
    id: string
    commandName: string
    argumentName?: string
    surfaces: Array<'query' | 'argument'>
    selectors: { terms?: string[]; digitRun?: { min: number; max: number } }
    limit: number
    timeoutMs: number
    index?: boolean
    bootstrapMigrationPaths?: string[]
  }>
  capabilities?: string[]
  id: string
  version: string
  replaces: string[]
  commands: NativeV2Command[]
  totalCommands: number
  status: 'complete' | 'partial' | 'unavailable'
  verification: NativeV2Verification[]
}

export type ExtensionInstallationStatus = 'installed' | 'not-installed' | 'needs-upgrade-or-repair'

export type ExtensionManifest = {
  /** Host catalog routing, outside the authenticated release manifest. */
  acquisition?: { type: 'remote' | 'local-import-only' | 'unsupported'; guidance?: string }
  installationStatus?: ExtensionInstallationStatus
  healthError?: string
  id: string
  name: string
  description: string
  author: string
  version: string
  repository?: string
  downloadCount?: number
  owner?: string
  icon?: string
  iconUrl?: string
  artwork?: string
  artworkBackground?: string
  authorIconUrl?: string
  screenshotUrls?: string[]
  categories?: string[]
  runtime?: ExtensionRuntimeKind
  nativeV2?: NativeV2Info
  platforms?: ExtensionPlatform[]
  commands?: Array<{
    name?: string
    title?: string
    description?: string
  }>
}

export type InstalledExtension = ExtensionManifest & {
  installedAt: number
}

/** Structured integrity report returned by the extension service. The UI
 *  uses this to surface a clear "reinstall required" state when the
 *  install pipeline detects tampering or partial downloads. */
export type ExtensionIntegrityReport = {
  extensionId: string
  installed: boolean
  commitRef?: string
  missingScripts: string[]
  tamperedScripts: string[]
  healthy: boolean
  lastError?: string
}
