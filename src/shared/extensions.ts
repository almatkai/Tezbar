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
  authorIconUrl?: string
  screenshotUrls?: string[]
  categories?: string[]
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
