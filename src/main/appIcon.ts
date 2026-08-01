import { app } from '@tezbar/desktop-runtime'
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, readdirSync } from 'node:fs'
import { basename, extname, join } from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
const appIconCache = new Map<string, string | null>()

/** Bundled fallback icons for well-known CLI tools that have no .app bundle. */
function bundledIconForPath(appPath: string): string | undefined {
  if (!appPath.toLowerCase().includes('brew')) return undefined
  const resourcesPath = (process as NodeJS.Process & { resourcesPath?: string }).resourcesPath
  const candidates = [
    // dev: src/main/ → resources/
    join(__dirname, '..', '..', 'resources', 'icons', 'homebrew.png'),
    // packaged: backend staged under app.localDataDir, icon beside it
    join(__dirname, 'resources', 'icons', 'homebrew.png'),
    join(process.cwd(), 'resources', 'icons', 'homebrew.png'),
    join(app.getAppPath(), 'resources', 'icons', 'homebrew.png'),
  ]
  if (resourcesPath) {
    candidates.push(join(resourcesPath, 'icons', 'homebrew.png'))
  }
  for (const p of candidates) {
    try {
      if (existsSync(p)) return `data:image/png;base64,${readFileSync(p).toString('base64')}`
    } catch {
      /* try next */
    }
  }
  return undefined
}

export async function appIconDataUrl(appPath: string): Promise<string | undefined> {
  if (appIconCache.has(appPath)) return appIconCache.get(appPath) ?? undefined
  try {
    if (process.platform === 'win32') {
      if (appPath.startsWith('shell:AppsFolder\\')) {
        appIconCache.set(appPath, null)
        return undefined
      }
      const cacheDir = join(app.getPath('userData'), 'icon-cache', 'applications')
      mkdirSync(cacheDir, { recursive: true })
      const pngPath = join(cacheDir, `${createHash('sha1').update(appPath).digest('hex')}-64.png`)
      if (!existsSync(pngPath)) {
        await execFileAsync(
          'powershell.exe',
          [
            '-NoLogo',
            '-NoProfile',
            '-NonInteractive',
            '-Command',
            "$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.Drawing; $icon=[System.Drawing.Icon]::ExtractAssociatedIcon($env:TEZBAR_ICON_SOURCE); if ($null -eq $icon) { exit 2 }; try { $bitmap=$icon.ToBitmap(); try { $bitmap.Save($env:TEZBAR_ICON_DEST,[System.Drawing.Imaging.ImageFormat]::Png) } finally { $bitmap.Dispose() } } finally { $icon.Dispose() }",
          ],
          {
            timeout: 5_000,
            windowsHide: true,
            env: {
              ...process.env,
              TEZBAR_ICON_SOURCE: appPath,
              TEZBAR_ICON_DEST: pngPath,
            },
          }
        )
      }
      const dataUrl = `data:image/png;base64,${readFileSync(pngPath).toString('base64')}`
      appIconCache.set(appPath, dataUrl)
      return dataUrl
    }

    const resourceDir = join(appPath, 'Contents', 'Resources')
    let iconName: string | undefined
    try {
      const { stdout } = await execFileAsync('/usr/bin/plutil', [
        '-extract',
        'CFBundleIconFile',
        'raw',
        '-o',
        '-',
        join(appPath, 'Contents', 'Info.plist'),
      ])
      const configured = stdout.trim()
      if (configured) iconName = extname(configured) ? configured : `${configured}.icns`
    } catch {
      // Older bundles may not declare CFBundleIconFile.
    }
    const resourceEntries = readdirSync(resourceDir)
    if (!iconName || !existsSync(join(resourceDir, iconName))) {
      const appName = basename(appPath, '.app').toLowerCase()
      iconName =
        resourceEntries.find((entry) => entry.toLowerCase() === `${appName}.icns`) ??
        resourceEntries.find((entry) => entry.toLowerCase().endsWith('.icns'))
    }
    if (!iconName) {
      const fallback = bundledIconForPath(appPath)
      appIconCache.set(appPath, fallback ?? null)
      return fallback
    }

    const iconPath = join(resourceDir, iconName)
    const cacheDir = join(app.getPath('userData'), 'icon-cache')
    mkdirSync(cacheDir, { recursive: true })
    const cacheName = `${createHash('sha1').update(iconPath).digest('hex')}-64.png`
    const pngPath = join(cacheDir, cacheName)

    if (!existsSync(pngPath)) {
      await execFileAsync('/usr/bin/sips', [
        '-Z',
        '64',
        '-s',
        'format',
        'png',
        iconPath,
        '--out',
        pngPath,
      ])
    }

    const dataUrl = `data:image/png;base64,${readFileSync(pngPath).toString('base64')}`
    appIconCache.set(appPath, dataUrl)
    return dataUrl
  } catch {
    const fallback = bundledIconForPath(appPath)
    appIconCache.set(appPath, fallback ?? null)
    return fallback
  }
}
