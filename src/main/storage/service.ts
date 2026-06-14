import { app, session } from 'electron'
import { lstat, readdir, rm, stat } from 'node:fs/promises'
import { join } from 'node:path'
import { getInstance } from '../search/indexDb'
import {
  clearClipboardImageHistory,
  getClipboardConfig,
  getClipboardImagesDir,
  getClipboardStoreDir,
  restartClipboardWatcher,
  setClipboardConfig,
  type ClipboardConfig,
} from '../search/providers/clipboardProvider'

export type StorageBreakdown = {
  totalBytes: number
  items: Array<{
    id: string
    label: string
    bytes: number
    paths: string[]
  }>
}

async function dirSize(root: string): Promise<number> {
  let total = 0
  const pending = [root]

  while (pending.length > 0) {
    const path = pending.pop()
    if (!path) continue
    try {
      const stats = await lstat(path)
      if (stats.isSymbolicLink()) continue
      if (stats.isFile()) {
        total += stats.size
        continue
      }
      if (stats.isDirectory()) {
        const entries = await readdir(path, { withFileTypes: true })
        for (const entry of entries) {
          pending.push(join(path, entry.name))
        }
      }
    } catch {
      // Files can disappear while Chromium or an extension is updating caches.
    }
  }

  return total
}

async function fileSize(path: string): Promise<number> {
  try {
    return (await stat(path)).size
  } catch {
    return 0
  }
}

function userData(...segments: string[]): string {
  return join(app.getPath('userData'), ...segments)
}

export async function getStorageBreakdown(): Promise<StorageBreakdown> {
  const searchDir = getClipboardStoreDir()
  const clipboardImagesDir = getClipboardImagesDir()
  const voiceModelsDir = userData('voice-models')
  const bunDir = userData('bun')
  const extensionsDir = userData('extensions')
  const cacheDir = userData('Cache')
  const codeCacheDir = userData('Code Cache')

  // Split search dir into DB/WAL/json and clipboard images for clarity.
  const [
    indexBytes,
    walBytes,
    shmBytes,
    clipboardJsonBytes,
    clipboardImagesBytes,
    voiceModelsBytes,
    bunBytes,
    extensionsBytes,
    cacheDirBytes,
    codeCacheBytes,
  ] = await Promise.all([
    fileSize(join(searchDir, 'index.sqlite3')),
    fileSize(join(searchDir, 'index.sqlite3-wal')),
    fileSize(join(searchDir, 'index.sqlite3-shm')),
    fileSize(join(searchDir, 'clipboard.json')),
    dirSize(clipboardImagesDir),
    dirSize(voiceModelsDir),
    dirSize(bunDir),
    dirSize(extensionsDir),
    dirSize(cacheDir),
    dirSize(codeCacheDir),
  ])
  const searchDbBytes = indexBytes + walBytes + shmBytes + clipboardJsonBytes
  const cacheBytes = cacheDirBytes + codeCacheBytes

  const items = [
    {
      id: 'clipboard-images',
      label: 'Clipboard images',
      bytes: clipboardImagesBytes,
      paths: [clipboardImagesDir],
    },
    {
      id: 'search-db',
      label: 'Search index & history',
      bytes: searchDbBytes,
      paths: [searchDir],
    },
    {
      id: 'voice-models',
      label: 'Voice models',
      bytes: voiceModelsBytes,
      paths: [voiceModelsDir],
    },
    { id: 'bun', label: 'Extension installer (Bun)', bytes: bunBytes, paths: [bunDir] },
    { id: 'extensions', label: 'Installed extensions', bytes: extensionsBytes, paths: [extensionsDir] },
    { id: 'chromium-cache', label: 'Chromium cache', bytes: cacheBytes, paths: [cacheDir, codeCacheDir] },
  ]

  return {
    totalBytes: items.reduce((sum, item) => sum + item.bytes, 0),
    items,
  }
}

export function getClipboardStorageConfig(): ClipboardConfig {
  return getClipboardConfig()
}

export function setClipboardStorageConfig(patch: Partial<ClipboardConfig>): void {
  setClipboardConfig(patch)
  // Polling interval and image-capture settings take effect immediately.
  restartClipboardWatcher()
}

export async function clearClipboardImages(): Promise<{ removed: number; freedBytes: number }> {
  return clearClipboardImageHistory()
}

export async function clearChromiumCache(): Promise<void> {
  const defaultSession = session.defaultSession
  if (!defaultSession) return
  await defaultSession.clearCache()
  await defaultSession.clearStorageData({ storages: ['shadercache'] })
  await Promise.all(
    ['Code Cache', 'GPUCache', 'DawnCache', 'GrShaderCache', 'ShaderCache'].map((name) =>
      rm(userData(name), { recursive: true, force: true }),
    ),
  )
}

export async function vacuumSearchDatabase(): Promise<{ beforeBytes: number; afterBytes: number }> {
  const searchDir = getClipboardStoreDir()
  const walPath = join(searchDir, 'index.sqlite3-wal')
  const beforeBytes = await fileSize(walPath)

  try {
    const db = getInstance()
    await db.ensureInitialized()
    // Truncate the WAL and reclaim free pages without rewriting the whole DB.
    db.vacuum()
  } catch (err) {
    console.warn('[storage] Search DB vacuum failed:', err)
  }

  const afterBytes = await fileSize(walPath)
  return { beforeBytes, afterBytes }
}
