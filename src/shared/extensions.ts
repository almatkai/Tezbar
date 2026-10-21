export type ExtensionPlatform = 'macOS' | 'Windows'
export type ExtensionRuntimeKind = 'rust' | 'mixed' | 'javascript'

export type NativeV2Command = {
  name: string
  title?: string
  runtime: 'rust' | 'javascript' | 'unavailable'
  platforms: ExtensionPlatform[]
  gap?: string
}

export type NativeV2Verification = {
  platform: ExtensionPlatform
  state: 'passed' | 'failed' | 'not-run'
  checkedAt?: number
  method?: 'automated' | 'manual'
  note?: string
}

export type NativeV2Info = {
  schemaVersion: 2
  id: string
  version: string
  replaces: string[]
  commands: NativeV2Command[]
  totalCommands: number
  status: 'complete' | 'partial' | 'unavailable'
  verification: NativeV2Verification[]
}

export type ExtensionManifest = {
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
