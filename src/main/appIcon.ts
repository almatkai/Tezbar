import { app } from 'electron'
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, readdirSync } from 'node:fs'
import { basename, extname, join } from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
const appIconCache = new Map<string, string | null>()

export async function appIconDataUrl(appPath: string): Promise<string | undefined> {
  if (appIconCache.has(appPath)) return appIconCache.get(appPath) ?? undefined
  try {
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
      appIconCache.set(appPath, null)
      return undefined
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
    appIconCache.set(appPath, null)
    return undefined
  }
}
