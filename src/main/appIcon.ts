import { app } from 'electron'
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
const appIconCache = new Map<string, string | null>()

export async function appIconDataUrl(appPath: string): Promise<string | undefined> {
  if (appIconCache.has(appPath)) return appIconCache.get(appPath) ?? undefined
  try {
    const resourceDir = join(appPath, 'Contents', 'Resources')
    const iconName = readdirSync(resourceDir).find((entry) => entry.toLowerCase().endsWith('.icns'))
    if (!iconName) {
      appIconCache.set(appPath, null)
      return undefined
    }

    const iconPath = join(resourceDir, iconName)
    const cacheDir = join(app.getPath('userData'), 'icon-cache')
    mkdirSync(cacheDir, { recursive: true })
    const cacheName = `${createHash('sha1').update(iconPath).digest('hex')}.png`
    const pngPath = join(cacheDir, cacheName)

    if (!existsSync(pngPath)) {
      await execFileAsync('/usr/bin/sips', ['-s', 'format', 'png', iconPath, '--out', pngPath])
    }

    const dataUrl = `data:image/png;base64,${readFileSync(pngPath).toString('base64')}`
    appIconCache.set(appPath, dataUrl)
    return dataUrl
  } catch {
    appIconCache.set(appPath, null)
    return undefined
  }
}
