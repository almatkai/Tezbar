export type ExtensionImageVariant = 'icon' | 'avatar' | 'screenshot' | 'artwork'
const EXTENSION_IMAGE_CACHE = 'tezbar-extension-images-v1'
type ImageEntry = {
  users: number
  source: Promise<string>
  objectUrl?: string
}
const images = new Map<string, ImageEntry>()

export type ExtensionImageLease = { source: Promise<string>; release: () => void }

export function extensionImageSource(value: string): string {
  if (/^(https?:|data:|file:|blob:)/i.test(value)) return value
  if (value.startsWith('/')) return `file://${encodeURI(value)}`
  return value
}

async function toWebpBlob(source: Blob, variant: ExtensionImageVariant): Promise<Blob> {
  const bitmap = await createImageBitmap(source)
  try {
    const maxWidth = variant === 'screenshot' ? 980 : variant === 'artwork' ? 640 : 128
    const scale = Math.min(1, maxWidth / Math.max(bitmap.width, 1))
    const width = Math.max(1, Math.round(bitmap.width * scale))
    const height = Math.max(1, Math.round(bitmap.height * scale))
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    const context = canvas.getContext('2d')
    if (!context) throw new Error('Canvas is unavailable')
    context.imageSmoothingEnabled = true
    context.imageSmoothingQuality = 'high'
    context.drawImage(bitmap, 0, 0, width, height)

    return await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (blob) => (blob ? resolve(blob) : reject(new Error('WebP encoding failed'))),
        'image/webp',
        variant === 'screenshot' ? 0.78 : 0.86
      )
    })
  } finally {
    bitmap.close()
  }
}

/** Share conversion work and Blob URLs only while a mounted image owns them.
 * CacheStorage remains the durable image cache, including offline revisits.
 */
export function acquireExtensionImage(
  src: string,
  variant: ExtensionImageVariant
): ExtensionImageLease {
  const normalized = extensionImageSource(src)
  const direct = (): ExtensionImageLease => ({
    source: Promise.resolve(normalized),
    release: () => undefined,
  })
  // Local assets must work without CacheStorage, fetch or canvas.
  if (!/^https?:\/\//i.test(normalized) || typeof window === 'undefined') return direct()
  try {
    if (typeof window.caches === 'undefined') return direct()
  } catch {
    return direct()
  }

  const key = `https://tezbar.local/extension-image-cache/${variant}/${encodeURIComponent(normalized)}.webp`
  let entry = images.get(key)
  if (!entry) {
    entry = { users: 0, source: Promise.resolve(normalized) }
    images.set(key, entry)
    entry.source = loadImage(normalized, variant, key, entry)
  }
  entry.users++
  const owned = entry
  let released = false
  return {
    source: owned.source,
    release: () => {
      if (released) return
      released = true
      if (--owned.users === 0) {
        if (images.get(key) === owned) images.delete(key)
        if (owned.objectUrl) URL.revokeObjectURL(owned.objectUrl)
      }
    },
  }
}

async function loadImage(
  normalized: string,
  variant: ExtensionImageVariant,
  key: string,
  entry: ImageEntry
): Promise<string> {
  try {
    const cache = await window.caches.open(EXTENSION_IMAGE_CACHE)
    const cached = await cache.match(key)
    let blob: Blob
    if (cached) {
      blob = await cached.blob()
    } else {
      const response = await fetch(normalized)
      if (!response.ok) throw new Error(`Image request failed: ${response.status}`)
      blob = await toWebpBlob(await response.blob(), variant)
      await cache.put(
        key,
        new Response(blob, {
          headers: { 'Content-Type': 'image/webp', 'Cache-Control': 'max-age=31536000, immutable' },
        })
      )
    }
    const source = URL.createObjectURL(blob)
    entry.objectUrl = source
    // A view can unmount while fetch/conversion is still in flight. Such a
    // completion must not retain a URL or replace a later acquisition's entry.
    if (entry.users === 0) URL.revokeObjectURL(source)
    return source
  } catch {
    return normalized
  }
}
