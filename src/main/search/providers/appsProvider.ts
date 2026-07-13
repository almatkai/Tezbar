import { readdirSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { homedir } from 'node:os'
import { join } from 'node:path'
import type { IndexedDocument, SearchProvider } from './types'

let windowsApplicationCache:
  | { collectedAt: number; applications: Array<{ name: string; path: string }> }
  | undefined

export function listApplications(): Array<{ name: string; path: string }> {
  if (process.platform === 'win32') {
    if (windowsApplicationCache && Date.now() - windowsApplicationCache.collectedAt < 30_000) {
      return windowsApplicationCache.applications
    }
    const roots = [
      join(
        process.env.ProgramData ?? 'C:\\ProgramData',
        'Microsoft',
        'Windows',
        'Start Menu',
        'Programs'
      ),
      join(
        process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming'),
        'Microsoft',
        'Windows',
        'Start Menu',
        'Programs'
      ),
    ]
    const out: Array<{ name: string; path: string }> = []
    const seen = new Set<string>()
    for (const root of roots) {
      try {
        for (const entry of readdirSync(root, { recursive: true, withFileTypes: true })) {
          if (!entry.isFile() || !entry.name.toLowerCase().endsWith('.lnk')) continue
          const name = entry.name.replace(/\.lnk$/i, '')
          if (seen.has(name.toLowerCase())) continue
          seen.add(name.toLowerCase())
          out.push({ name, path: join(entry.parentPath, entry.name) })
        }
      } catch {
        // Ignore inaccessible Start Menu folders.
      }
    }
    try {
      const raw = execFileSync(
        'powershell.exe',
        [
          '-NoLogo',
          '-NoProfile',
          '-NonInteractive',
          '-Command',
          'Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress',
        ],
        { encoding: 'utf8', timeout: 5_000, windowsHide: true }
      ).trim()
      const parsed = raw ? (JSON.parse(raw) as unknown) : []
      const entries = Array.isArray(parsed) ? parsed : [parsed]
      for (const value of entries) {
        if (!value || typeof value !== 'object') continue
        const item = value as { Name?: unknown; AppID?: unknown }
        if (typeof item.Name !== 'string' || typeof item.AppID !== 'string') continue
        const name = item.Name.trim()
        const appId = item.AppID.trim()
        if (!name || !appId || seen.has(name.toLowerCase())) continue
        seen.add(name.toLowerCase())
        out.push({ name, path: `shell:AppsFolder\\${appId}` })
      }
    } catch {
      // Shortcut discovery still provides classic desktop applications when
      // Get-StartApps is unavailable or blocked by system policy.
    }
    windowsApplicationCache = { collectedAt: Date.now(), applications: out }
    return out
  }
  const roots = [
    '/Applications',
    '/Applications/Utilities',
    '/System/Applications',
    '/System/Applications/Utilities',
    '/System/Library/CoreServices/Applications',
    '/System/Library/CoreServices',
    join(homedir(), 'Applications'),
  ]
  const out: Array<{ name: string; path: string }> = []
  const seen = new Set<string>()

  for (const root of roots) {
    try {
      for (const entry of readdirSync(root)) {
        if (!entry.endsWith('.app')) continue
        const name = entry.replace(/\.app$/, '')
        if (seen.has(name)) continue
        seen.add(name)
        out.push({
          name,
          path: join(root, entry),
        })
      }
    } catch {
      // Ignore inaccessible roots.
    }
  }

  return out
}

export const appsProvider: SearchProvider = {
  providerId: 'apps',
  async buildDocuments(): Promise<IndexedDocument[]> {
    const now = Date.now()
    return listApplications().map((app) => ({
      id: `app:${app.path}`,
      category: 'applications',
      title: app.name,
      subtitle: app.path,
      tokens: `${app.name} ${app.path}`,
      action: { type: 'open-app', appName: app.name, appPath: app.path },
      updatedAt: now,
      sourcePath: app.path,
    }))
  },
}
