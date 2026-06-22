import { app } from 'electron'
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, statSync } from 'node:fs'
import { basename, extname, join } from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

type FileIconStyle = { label: string; color: string }

const FILE_ICON_STYLES: Record<string, FileIconStyle> = {
  '.c': { label: 'C', color: '#6b8dd6' },
  '.cpp': { label: 'C++', color: '#5e74c9' },
  '.css': { label: 'CSS', color: '#4a90e2' },
  '.go': { label: 'GO', color: '#45b8d8' },
  '.html': { label: 'HTML', color: '#e66b3d' },
  '.java': { label: 'JAVA', color: '#d95d54' },
  '.js': { label: 'JS', color: '#e5c441' },
  '.jsx': { label: 'JSX', color: '#5fc9e8' },
  '.json': { label: '{}', color: '#d2b84c' },
  '.kt': { label: 'KT', color: '#8c6bd1' },
  '.md': { label: 'MD', color: '#778195' },
  '.pdf': { label: 'PDF', color: '#df5b5b' },
  '.php': { label: 'PHP', color: '#777bb3' },
  '.py': { label: 'PY', color: '#4d8fbd' },
  '.rb': { label: 'RB', color: '#c95151' },
  '.rs': { label: 'RS', color: '#c7764d' },
  '.scss': { label: 'SASS', color: '#cc6699' },
  '.sh': { label: '>_', color: '#58a36b' },
  '.sql': { label: 'SQL', color: '#527fa5' },
  '.swift': { label: 'SW', color: '#ef704f' },
  '.ts': { label: 'TS', color: '#3178c6' },
  '.tsx': { label: 'TSX', color: '#4ba6c8' },
  '.txt': { label: 'TXT', color: '#7d8798' },
  '.xml': { label: 'XML', color: '#d58945' },
  '.yaml': { label: 'YML', color: '#c85a67' },
  '.yml': { label: 'YML', color: '#c85a67' },
  '.zip': { label: 'ZIP', color: '#a78a55' },
}

const IMAGE_EXTENSIONS = new Set([
  '.avif',
  '.bmp',
  '.gif',
  '.heic',
  '.jpeg',
  '.jpg',
  '.png',
  '.svg',
  '.webp',
])
const ARCHIVE_EXTENSIONS = new Set(['.7z', '.bz2', '.gz', '.rar', '.tar', '.tgz'])
const nativeFileIconCache = new Map<string, string | null>()

function svgDataUrl(svg: string): string {
  return `data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`
}

function documentSvg(label: string, color: string): string {
  const safeLabel = label.replace(/[&<>"']/g, '')
  return `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><path fill="#f5f6f8" d="M13 5h25l13 13v41H13z"/><path fill="#d9dde5" d="M38 5v14h13z"/><rect x="17" y="36" width="30" height="17" rx="4" fill="${color}"/><text x="32" y="48" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="10" font-weight="800" fill="white">${safeLabel}</text></svg>`
}

export const folderIconDataUrl = svgDataUrl(
  '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><path fill="#62a8ed" d="M5 15a6 6 0 0 1 6-6h15l6 7h21a6 6 0 0 1 6 6v29a6 6 0 0 1-6 6H11a6 6 0 0 1-6-6z"/><path fill="#8bc4f7" d="M5 25h54v26a6 6 0 0 1-6 6H11a6 6 0 0 1-6-6z"/></svg>'
)

export function fileIconDataUrl(path: string): string {
  const extension = extname(path).toLowerCase()
  const style = FILE_ICON_STYLES[extension]
  if (style) return svgDataUrl(documentSvg(style.label, style.color))
  if (IMAGE_EXTENSIONS.has(extension)) return svgDataUrl(documentSvg('IMG', '#8b6fc0'))
  if (ARCHIVE_EXTENSIONS.has(extension)) return svgDataUrl(documentSvg('ZIP', '#a78a55'))
  return svgDataUrl(
    documentSvg(extension ? extension.slice(1, 5).toUpperCase() : 'FILE', '#7d8798')
  )
}

export function imageFileDataUrl(path: string): string | undefined {
  if (!existsSync(path)) return undefined
  const mimeType =
    extname(path).toLowerCase() === '.svg'
      ? 'image/svg+xml'
      : extname(path).toLowerCase() === '.jpg' || extname(path).toLowerCase() === '.jpeg'
        ? 'image/jpeg'
        : extname(path).toLowerCase() === '.webp'
          ? 'image/webp'
          : 'image/png'
  try {
    return `data:${mimeType};base64,${readFileSync(path).toString('base64')}`
  } catch {
    return undefined
  }
}

export async function nativeFileIconDataUrl(path: string): Promise<string | undefined> {
  if (nativeFileIconCache.has(path)) return nativeFileIconCache.get(path) ?? undefined
  if (!existsSync(path)) return undefined

  try {
    const stats = statSync(path)
    const cacheKey = createHash('sha1')
      .update(`${path}:${stats.mtimeMs}:${stats.size}`)
      .digest('hex')
    const outputDir = join(app.getPath('userData'), 'icon-cache', 'files', cacheKey)
    const outputPath = join(outputDir, `${basename(path)}.png`)
    mkdirSync(outputDir, { recursive: true })

    if (!existsSync(outputPath)) {
      await execFileAsync('/usr/bin/qlmanage', ['-t', '-i', '-s', '64', '-o', outputDir, path], {
        timeout: 3_000,
      })
    }
    if (!existsSync(outputPath)) {
      nativeFileIconCache.set(path, null)
      return undefined
    }

    const dataUrl = `data:image/png;base64,${readFileSync(outputPath).toString('base64')}`
    nativeFileIconCache.set(path, dataUrl)
    return dataUrl
  } catch {
    nativeFileIconCache.set(path, null)
    return undefined
  }
}
