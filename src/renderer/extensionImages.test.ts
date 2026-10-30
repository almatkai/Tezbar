import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  acquireExtensionImage,
  extensionImageSource,
  type ExtensionImageVariant,
} from './extensionImages'

async function cachedExtensionImageSource(
  src: string,
  variant: ExtensionImageVariant
): Promise<string> {
  const image = acquireExtensionImage(src, variant)
  try {
    return await image.source
  } finally {
    image.release()
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('extension images', () => {
  it('keeps a shared blob alive for mounted consumers and revokes it after the last release', async () => {
    const open = vi
      .fn()
      .mockResolvedValue({
        match: vi.fn().mockImplementation(async () => new Response(new Blob(['webp']))),
      })
    vi.stubGlobal('window', { caches: { open } })
    vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:owned-lifecycle')
    const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined)
    const source = 'https://example.com/ownership-icon.png'
    const first = acquireExtensionImage(source, 'icon')
    await first.source
    const second = acquireExtensionImage(source, 'icon')
    await second.source
    first.release()
    expect(revoke).not.toHaveBeenCalled()
    second.release()
    expect(revoke).toHaveBeenCalledExactlyOnceWith('blob:owned-lifecycle')
    second.release()
    expect(revoke).toHaveBeenCalledTimes(1)
    expect(open).toHaveBeenCalledOnce()
    const revisit = acquireExtensionImage(source, 'icon')
    expect(await revisit.source).toBe('blob:owned-lifecycle')
    expect(open).toHaveBeenCalledTimes(2)
    revisit.release()
    expect(revoke).toHaveBeenCalledTimes(2)
  })
  it('coalesces in-flight consumers and releases a URL that finishes after unmount', async () => {
    let complete!: (value: Response) => void
    const match = vi.fn().mockImplementation(
      () =>
        new Promise<Response>((resolve) => {
          complete = resolve
        })
    )
    vi.stubGlobal('window', { caches: { open: vi.fn().mockResolvedValue({ match }) } })
    vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:late-image')
    const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined)
    const source = 'https://example.com/late-icon.png'
    const first = acquireExtensionImage(source, 'icon')
    const second = acquireExtensionImage(source, 'icon')
    await Promise.resolve()
    expect(match).toHaveBeenCalledOnce()
    first.release()
    second.release()
    complete(new Response(new Blob(['webp'])))
    await Promise.all([first.source, second.source])
    expect(revoke).toHaveBeenCalledExactlyOnceWith('blob:late-image')
  })
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
    vi.stubGlobal('window', {
      get caches() {
        throw new Error('CacheStorage unavailable')
      },
    })
    expect(await cachedExtensionImageSource('/fixture/my icon.png', 'icon')).toBe(
      'file:///fixture/my%20icon.png'
    )
    for (const source of [
      'data:image/png;base64,AA==',
      'file:///fixture/icon.png',
      'blob:fixture',
    ]) {
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
    expect(match).toHaveBeenCalledWith(
      `https://tezbar.local/extension-image-cache/icon/${encodeURIComponent(source)}.webp`
    )
    expect(objectUrl).toHaveBeenCalledOnce()
    expect(fetch).not.toHaveBeenCalled()
  })
})
