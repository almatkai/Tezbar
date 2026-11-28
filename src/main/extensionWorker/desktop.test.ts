import { afterEach, describe, expect, it, vi } from 'vitest'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const host = vi.hoisted(() => ({
  request: vi.fn(async () => ({ ok: true })),
  notify: vi.fn(),
  send: vi.fn(),
}))
vi.mock('./host', () => host)
import { clipboard, nativeImage } from './desktop'

const paths: string[] = []
afterEach(() => {
  for (const path of paths.splice(0)) rmSync(path, { recursive: true, force: true })
  vi.clearAllMocks()
})

describe('worker image clipboard transport', () => {
  it('sends PNG buffers as exact PNG data URLs', async () => {
    const png = Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aRZkAAAAASUVORK5CYII=',
      'base64'
    )
    const image = nativeImage.createFromBuffer(png)
    expect(image.getSize()).toEqual({ width: 1, height: 1 })
    await clipboard.writeImage(image)
    expect(host.request).toHaveBeenCalledWith(
      'clipboard:write-png',
      `data:image/png;base64,${png.toString('base64')}`
    )
  })

  it('keeps original file formats for the native host to decode', async () => {
    const root = mkdtempSync(join(tmpdir(), 'tezbar-image-'))
    paths.push(root)
    const path = join(root, 'image.jpg')
    writeFileSync(path, Buffer.from([0xff, 0xd8, 0xff, 0xd9]))
    await clipboard.writeImage(nativeImage.createFromPath(path))
    expect(host.request).toHaveBeenCalledWith('core:platform:clipboard-write', { file: path })
  })

  it('rejects non-PNG buffers instead of labelling JPEG bytes as PNG', async () => {
    await expect(
      clipboard.writeImage(nativeImage.createFromBuffer(Buffer.from([0xff, 0xd8])))
    ).rejects.toThrow('PNG encoding is required')
    expect(host.request).not.toHaveBeenCalled()
  })
})
