import { afterEach, describe, expect, it, vi } from 'vitest'
import { cachedExtensionImageSource, extensionImageSource } from './extensionImages'

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('extension images', () => {
  it('preserves local and data sources without HTTP cache or conversion', async () => {
    const open = vi.fn()
    const fetch = vi.fn()
    const createImageBitmap = vi.fn()
    vi.stubGlobal('window', { caches: { open } })
    vi.stubGlobal('fetch', fetch)
    vi.stubGlobal('createImageBitmap', createImageBitmap)
    const dataUri = 'data:image/png;base64,AA=='
    expect(extensionImageSource(dataUri)).toBe(dataUri)
    expect(await cachedExtensionImageSource(dataUri, 'icon')).toBe(dataUri)
    expect(open).not.toHaveBeenCalled()
    expect(fetch).not.toHaveBeenCalled()
    expect(createImageBitmap).not.toHaveBeenCalled()
  })

  it('preserves local sources without requiring CacheStorage, including legacy filesystem icons', async () => {
    vi.stubGlobal('window', { get caches() { throw new Error('CacheStorage unavailable') } })
    expect(await cachedExtensionImageSource('/fixture/my icon.png', 'icon')).toBe('file:///fixture/my%20icon.png')
    for (const source of ['data:image/png;base64,AA==', 'file:///fixture/icon.png', 'blob:fixture']) {
      expect(await cachedExtensionImageSource(source, 'icon')).toBe(source)
    }
  })

  it('still reads cached remote images without refetching or converting them', async () => {
    const blob = new Blob(['webp'], { type: 'image/webp' })
    const match = vi.fn().mockResolvedValue(new Response(blob))
    const open = vi.fn().mockResolvedValue({ match })
    const fetch = vi.fn()
    vi.stubGlobal('window', { caches: { open } })
    vi.stubGlobal('fetch', fetch)
    const objectUrl = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:cached-extension')
    const source = 'https://example.com/cached-icon.png'
    expect(await cachedExtensionImageSource(source, 'icon')).toBe('blob:cached-extension')
    expect(open).toHaveBeenCalledWith('tezbar-extension-images-v1')
    expect(match).toHaveBeenCalledWith(`https://tezbar.local/extension-image-cache/icon/${encodeURIComponent(source)}.webp`)
    expect(objectUrl).toHaveBeenCalledOnce()
    expect(fetch).not.toHaveBeenCalled()
  })
})
