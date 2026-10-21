export type ExtensionImageVariant = 'icon' | 'avatar' | 'screenshot' | 'artwork'
const EXTENSION_IMAGE_CACHE = 'tezbar-extension-images-v1'
const imageObjectUrls = new Map<string, string>()

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
        variant === 'screenshot' ? 0.78 : 0.86,
      )
    })
  } finally {
    bitmap.close()
  }
}

export async function cachedExtensionImageSource(src: string, variant: ExtensionImageVariant): Promise<string> {
  const normalized = extensionImageSource(src)
  // Local assets must work offline, without CacheStorage, fetch or canvas.
  if (!/^https?:\/\//i.test(normalized) || typeof window === 'undefined' || typeof window.caches === 'undefined') {
    return normalized
  }

  const key = `https://tezbar.local/extension-image-cache/${variant}/${encodeURIComponent(normalized)}.webp`
  const cachedObjectUrl = imageObjectUrls.get(key)
  if (cachedObjectUrl) return cachedObjectUrl

  try {
    const cache = await window.caches.open(EXTENSION_IMAGE_CACHE)
    const cached = await cache.match(key)
    if (cached) {
      const objectUrl = URL.createObjectURL(await cached.blob())
      imageObjectUrls.set(key, objectUrl)
      return objectUrl
    }

    const response = await fetch(normalized)
    if (!response.ok) throw new Error(`Image request failed: ${response.status}`)
    const webp = await toWebpBlob(await response.blob(), variant)
    await cache.put(
      key,
      new Response(webp, {
        headers: {
          'Content-Type': 'image/webp',
          'Cache-Control': 'max-age=31536000, immutable',
        },
      }),
    )
    const objectUrl = URL.createObjectURL(webp)
    imageObjectUrls.set(key, objectUrl)
    return objectUrl
  } catch {
    return normalized
  }
}
